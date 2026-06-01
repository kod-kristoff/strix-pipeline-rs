use counter::Counter;
use exn::{Exn, ResultExt};
use fs_err as fs;
use hashbrown::{HashMap, HashSet};
use itertools::Itertools;
use quick_xml::events::{Event, attributes::Attributes};
use serde_json::{Map, Value, json};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    io::BufReader,
    path::{Path, PathBuf},
};

use crate::mappingutil;

pub struct ParsePipelineXmlOptions<'a> {
    pub struct_annotations: Cow<'a, serde_json::Map<String, Value>>,
    pub token_count_id: bool,
    pub text_attributes: Cow<'a, serde_json::Map<String, Value>>,
    pub process_token: Box<dyn Fn(&Map<String, Value>) -> Option<String>>,
    pub add_most_common_words: bool,
    pub save_whitespace_per_token: bool,
    pub pos_index_attributes: Vec<String>,
    pub text_tags: Cow<'a, Vec<String>>,
}

impl<'a> Default for ParsePipelineXmlOptions<'a> {
    fn default() -> Self {
        Self {
            struct_annotations: Cow::Owned(serde_json::Map::new()),
            token_count_id: false,
            text_attributes: Cow::Owned(serde_json::Map::new()),
            process_token: Box::new(|_x| None),
            add_most_common_words: false,
            save_whitespace_per_token: false,
            pos_index_attributes: Vec::new(),
            text_tags: Cow::Owned(Vec::new()),
        }
    }
}
pub fn parse_pipeline_xml(
    file_name: &Path,
    split_document: &str,
    word_annotations: &serde_json::Map<String, Value>,
    options: ParsePipelineXmlOptions,
) -> Result<Vec<Part>, Exn<ParseXmlError>> {
    let mut strix_parser = StrixParser::new(
        split_document,
        word_annotations,
        &options.struct_annotations,
        options.token_count_id,
        &options.text_attributes,
        options.process_token,
        options.add_most_common_words,
        options.save_whitespace_per_token,
        &options.pos_index_attributes,
        &options.text_tags,
    );
    iterparse_parser(file_name, &mut strix_parser).or_raise(|| ParseXmlError)?;
    let res = strix_parser.get_result();
    Ok(res)
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to parse xml")]
pub struct ParseXmlError;

struct StrixParser<'a> {
    // input
    split_document: &'a str,
    word_annotations: &'a serde_json::Map<String, Value>,
    struct_annotations: &'a serde_json::Map<String, Value>,
    token_count_id: bool,
    text_attributes: &'a serde_json::Map<String, Value>,
    process_token: Box<dyn Fn(&serde_json::Map<String, Value>) -> Option<String>>,
    add_most_common_words: bool,
    save_whitespace_per_token: bool,
    pos_index_attributes: &'a Vec<String>,
    text_tags: &'a Vec<String>,
    // state
    current_part_tokens: Vec<HashMap<String, String>>,
    current_word_annotations: serde_json::Map<String, Value>,
    all_word_level_annotations: HashSet<String>,
    current_struct_annotations: serde_json::Map<String, Value>,
    part_attributes: serde_json::Map<String, Value>,
    upper_level: serde_json::Map<String, Value>,

    current_token_lookup: Vec<TokenLookup>,
    dump: Vec<String>,
    token_count: i64,
    lines: Vec<Vec<i64>>,
    most_common_words: Vec<String>,
    ner_tags: Vec<String>,
    geo_locations: Vec<String>,

    in_word: bool,
    word_attrs: HashMap<String, String>,
    current_word_content: String,

    current_parts: Vec<Part>,
    start_tag: Vec<u8>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Part {
    pub text_attributes: serde_json::Map<String, Value>,
    token_lookup: Vec<TokenLookup>,
    pub dump: Vec<String>,
    lines: Vec<Vec<i64>>,
    word_count: usize,
    text: String,
    ner_tags: String,
    geo_locations: Vec<String>,
    most_common_words: String,
    wid: String,
    #[serde(flatten)]
    extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TokenLookup {
    word: String,
    attrs: serde_json::Map<String, Value>,
    position: usize,
    whitespace: Option<char>,
}

impl<'a> StrixParser<'a> {
    pub fn new(
        split_document: &'a str,
        word_annotations: &'a serde_json::Map<String, Value>,
        struct_annotations: &'a serde_json::Map<String, Value>,
        token_count_id: bool,
        text_attributes: &'a serde_json::Map<String, Value>,
        process_token: Box<dyn Fn(&serde_json::Map<String, Value>) -> Option<String>>,
        add_most_common_words: bool,
        save_whitespace_per_token: bool,
        pos_index_attributes: &'a Vec<String>,
        text_tags: &'a Vec<String>,
    ) -> Self {
        Self {
            split_document,
            word_annotations,
            struct_annotations,
            token_count_id,
            text_attributes,
            process_token,
            add_most_common_words,
            save_whitespace_per_token,
            pos_index_attributes,
            text_tags,
            // state
            part_attributes: serde_json::Map::new(),
            upper_level: serde_json::Map::new(),
            current_part_tokens: Vec::new(),
            current_word_annotations: serde_json::Map::new(),
            all_word_level_annotations: HashSet::new(),
            current_struct_annotations: serde_json::Map::new(),

            current_token_lookup: Vec::new(),
            dump: vec![String::from("")],
            token_count: 0,
            lines: vec![vec![0]],
            most_common_words: Vec::new(),
            ner_tags: Vec::new(),
            geo_locations: Vec::new(),

            in_word: false,
            word_attrs: HashMap::new(),
            current_word_content: String::new(),

            current_parts: Vec::new(),
            start_tag: Vec::new(),
        }
    }

    pub fn get_result(self) -> Vec<Part> {
        self.current_parts
    }

    fn handle_starttag<'e>(
        &mut self,
        tag: &[u8],
        attrs: HashMap<String, String>,
    ) -> Result<(), Exn<StrixParserError>> {
        let make_error = || StrixParserError;
        let tag_str = String::from_utf8_lossy(tag).to_string();
        if !self.text_attributes.is_empty() && self.text_tags.contains(&tag_str) {
            if self.start_tag.is_empty() {
                self.start_tag = tag.to_vec();
                self.upper_level = serde_json::Map::new();
            }
            if tag_str == self.split_document {
                self.part_attributes = Map::new();
                for (text_attr, text_attr_obj) in self.text_attributes {
                    dbg!(&text_attr);
                    dbg!(&text_attr_obj);
                    for (attr_name, attr_value) in &attrs {
                        let attr_value: Value = Value::String(attr_value.into());
                        let (mut text_attr_value, node_name, new_name) = if attr_name == text_attr {
                            (attr_value, attr_name.clone(), attr_name.clone())
                        } else if text_attr_obj["nodeName"]
                            .as_str()
                            .map(|s| s == attr_name)
                            .unwrap_or(false)
                        {
                            (attr_value, attr_name.clone(), text_attr.clone())
                        } else {
                            continue;
                        };

                        let is_set = self
                            .text_attributes
                            .get(&new_name)
                            .and_then(|v| v.get("set").map(|v| v.as_bool()))
                            .flatten()
                            .unwrap_or(false);
                        let starts_with_pipe = text_attr_value
                            .as_str()
                            .map(|s| s.starts_with('|'))
                            .unwrap_or(false);
                        let ends_with_pipe = text_attr_value
                            .as_str()
                            .map(|s| s.ends_with('|'))
                            .unwrap_or(false);
                        if is_set || (starts_with_pipe && ends_with_pipe) {
                            text_attr_value =
                                text_attr_value.as_str().unwrap().split('|').collect();
                        }
                        let type_is_double = self
                            .text_attributes
                            .get(&new_name)
                            .and_then(|v| v.get("type").map(|v| v.as_str().unwrap() == "double"))
                            .unwrap_or(false);
                        if type_is_double {
                            text_attr_value = if text_attr_value.as_str().unwrap() == "inf" {
                                "Infinity".into()
                            } else {
                                text_attr_value
                            };
                        }
                        self.part_attributes.insert(new_name, text_attr_value);
                    }
                }
                for (key, value) in &self.upper_level {
                    self.part_attributes.insert(key.clone(), value.clone());
                }
            } else {
                // if tag != self.split_document
                for (text_attr, text_attr_obj) in self.text_attributes.iter() {
                    for (attr_name, attr_value) in &attrs {
                        let tag_attribute = format!("{}_{}", tag_str, attr_name);
                        if &tag_attribute == text_attr
                            || text_attr_obj
                                .get("nodeName")
                                .map(|v| v.as_str().unwrap() == tag_attribute)
                                .unwrap_or(false)
                        {
                        } else {
                            continue;
                        }
                        let new_name = text_attr.clone();
                        let mut text_attr_value = Value::String(attr_value.into());
                        let is_set = self
                            .text_attributes
                            .get(&new_name)
                            .and_then(|v| v.get("set").map(|v| v.as_bool()))
                            .flatten()
                            .unwrap_or(false);
                        let starts_with_pipe = text_attr_value
                            .as_str()
                            .map(|s| s.starts_with('|'))
                            .unwrap_or(false);
                        let ends_with_pipe = text_attr_value
                            .as_str()
                            .map(|s| s.ends_with('|'))
                            .unwrap_or(false);
                        if is_set || (starts_with_pipe && ends_with_pipe) {
                            text_attr_value =
                                text_attr_value.as_str().unwrap().split('|').collect();
                        }
                        let type_is_double = self
                            .text_attributes
                            .get(&new_name)
                            .and_then(|v| v.get("type").map(|v| v.as_str().unwrap() == "double"))
                            .unwrap_or(false);
                        if type_is_double {
                            text_attr_value = if text_attr_value.as_str().unwrap() == "inf" {
                                "Infinity".into()
                            } else {
                                text_attr_value
                            };
                        }
                        self.upper_level.insert(new_name, text_attr_value);
                    }
                }
            }
        } else if tag_str == "token" {
            self.in_word = true;
            self.word_attrs = attrs;
        } else if self.struct_annotations.contains_key(&tag_str) {
            todo!()
        } else if tag_str != "token" && self.word_annotations.contains_key(&tag_str) {
            todo!()
        }
        Ok(())
    }
    fn handle_endtag<'e>(&mut self, tag: &[u8]) -> Result<(), Exn<StrixParserError>> {
        let make_error = || StrixParserError;

        let tag_str = String::from_utf8_lossy(tag).to_string();
        if tag_str == self.split_document {
            let mut current_part = Part::default();
            if !self.text_attributes.is_empty() {
                if !self.part_attributes.contains_key("year") {
                    // TODO do not augment data inside XML-parser
                    let mut date_from = "";
                    let mut date_to = "";
                    let mut given_date = "";
                    if self.part_attributes.contains_key("datefrom") {
                        date_from = &self.part_attributes["datefrom"].as_str().unwrap()[0..4];
                    }
                    if self.part_attributes.contains_key("dateto") {
                        date_to = &self.part_attributes["dateto"].as_str().unwrap()[0..4];
                    }

                    if self.part_attributes.contains_key("date") {
                        given_date = &self.part_attributes["date"].as_str().unwrap()[0..4];
                    } else if self.part_attributes.contains_key("datum") {
                        given_date = &self.part_attributes["datum"].as_str().unwrap()[0..4];
                    } else if self.part_attributes.contains_key("topic_year") {
                        given_date = &self.part_attributes["topic_year"].as_str().unwrap()[0..4];
                    }

                    // TODO find permament solution
                    let year =
                        if date_from.is_empty() && (date_to.is_empty() && given_date.is_empty()) {
                            Some(Value::String("2050".into()))
                        } else if !given_date.is_empty() {
                            Some(Value::String(given_date.into()))
                        } else if !date_to.is_empty() && date_from.is_empty() {
                            Some(Value::String(date_to.into()))
                        } else if !date_from.is_empty() && date_to.is_empty() {
                            Some(Value::String(date_from.into()))
                        } else if date_from == date_to {
                            Some(Value::String(date_from.into()))
                        } else if date_from != date_to {
                            Some(Value::String(format!("{date_from}, {date_to}")))
                        } else {
                            None
                        };
                    if let Some(year) = year {
                        self.part_attributes.insert(String::from("year"), year);
                    }
                }

                for (key, val) in &self.part_attributes {
                    if self
                        .text_attributes
                        .get(key)
                        .map(|v| v.get("index").and_then(|v| v.as_bool()).unwrap_or(true))
                        .unwrap_or(false)
                    {
                        current_part
                            .extra
                            .insert(format!("text_{val}"), val.clone());
                    }
                }
                current_part.text_attributes = self.part_attributes.clone();
            }
            current_part.token_lookup = self.current_token_lookup.clone();

            if self.lines.last().unwrap().len() == 1 && self.lines.last().unwrap()[0] != -1 {
                self.lines
                    .last_mut()
                    .unwrap()
                    .push(self.token_count as i64 - 1);
            }
            current_part.dump = self.dump.clone();
            current_part.lines = self.lines.clone();

            current_part.word_count = self.current_part_tokens.len();

            current_part.text = self
                .current_part_tokens
                .iter()
                .map(|x| x.get("token"))
                .flatten()
                .join(mappingutil::TOKEN_SEPARATOR);

            for key in &self.all_word_level_annotations {
                let res = self
                    .current_part_tokens
                    .iter()
                    .map(|x| {
                        x.get(key)
                            .map(|s| s.as_str())
                            .unwrap_or(mappingutil::EMPTY_SET)
                    })
                    .join(mappingutil::TOKEN_SEPARATOR);
                if key == "wid" {
                    current_part.wid = res;
                } else if self.pos_index_attributes.contains(key) {
                    current_part.extra.insert(format!("pos_{key}"), res.into());
                }
            }

            if !self.ner_tags.is_empty() {
                current_part.ner_tags = self
                    .ner_tags
                    .iter()
                    .filter(|i| i.len() > 3)
                    .collect::<Counter<_>>()
                    .k_most_common_ordered(10)
                    .iter()
                    .map(|(key, value)| format!("{key} ({value})"))
                    .join(", ");
            }

            if !self.geo_locations.is_empty() {
                current_part.geo_locations = self.geo_locations.clone();
            }

            if self.add_most_common_words {
                current_part.most_common_words = self
                    .most_common_words
                    .iter()
                    .filter(|i| i.len() > 3)
                    .collect::<Counter<_>>()
                    .k_most_common_ordered(10)
                    .iter()
                    .map(|(key, value)| format!("{key} ({value})"))
                    .join(", ");
            }
            self.current_parts.push(current_part);

            self.token_count = 0;
            self.current_part_tokens.clear();
            self.current_token_lookup.clear();
            self.dump = vec![String::from("")];
            self.lines = vec![vec![0]];
            self.most_common_words.clear();
            self.ner_tags.clear();
            self.geo_locations.clear();
            self.all_word_level_annotations.clear();
        } else if self.struct_annotations.contains_key(&tag_str) {
            // at close we go thorugh each <w>-tag in the structural element and
            // assign the length (which can't be known until the element closes)
            // TODO do this once for ALL structural elements to avoid editing each token more than one
            //   (save all structs and do this when the document is done)
            if let Some(annotation_length) = self.current_struct_annotations[&tag_str]
                .get("length")
                .and_then(|l| l.as_u64())
            {
                let skip = self.current_token_lookup.len() - annotation_length as usize;
                for token in self.current_token_lookup.iter_mut().skip(skip) {
                    token
                        .attrs
                        .entry(&tag_str)
                        .or_insert_with(|| json!({}))
                        .as_object_mut()
                        .unwrap()
                        .insert("length".into(), annotation_length.into());
                }
            }
            self.current_struct_annotations.remove(&tag_str);
        } else if tag_str == "token" {
            let token = self.current_word_content.trim();
            if !token.is_empty() {
                let mut token_data = self.current_word_annotations.clone();
                if let Some(annotations) = self.word_annotations.get("token") {
                    for annotation in annotations.as_array().unwrap() {
                        let annotation_name = &annotation["name"];
                        let annotation_value: String = if let Some(node_name) =
                            annotation.as_object().unwrap().get("nodeName")
                        {
                            let mut annotation_value = Vec::new();
                            for lemma in self
                                .word_attrs
                                .get(node_name.as_str().unwrap())
                                .unwrap()
                                .split('|')
                            {
                                if !lemma.is_empty() && !lemma.contains('.') {
                                    annotation_value.push(lemma.to_string());
                                } else if lemma.contains(':') {
                                    annotation_value
                                        .push(lemma.split_once(':').unwrap().0.to_string());
                                }
                            }
                            if annotation_value.is_empty() {
                                String::new()
                            } else {
                                annotation_value.join("|")
                            }
                        } else {
                            self.word_attrs
                                .get(annotation_name.as_str().unwrap())
                                .unwrap()
                                .to_string()
                        };
                        let mut annotation_value: Value = Value::String(annotation_value);
                        if annotation
                            .get("set")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                            || annotation
                                .get("ranked")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                        {
                            annotation_value = annotation_value
                                .as_str()
                                .unwrap()
                                .split('|')
                                .filter(|v| !v.is_empty())
                                .collect();
                        }
                        if annotation
                            .get("ranked")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                        {
                            let values: Vec<&str> = annotation_value
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| v.as_str().unwrap().split(':').into_iter().nth(0).unwrap())
                                .collect();
                            annotation_value = if values.is_empty() {
                                Value::Null
                            } else {
                                Value::String(values[0].to_string())
                            };
                        }
                        token_data.insert(
                            annotation_name.as_str().unwrap().to_string(),
                            annotation_value,
                        );
                        self.all_word_level_annotations
                            .insert(annotation_name.as_str().unwrap().to_string());
                    }
                }

                if self.token_count_id {
                    token_data.insert("wid".into(), self.token_count.into());
                    self.all_word_level_annotations.insert("wid".into());
                }

                let mut struct_data = serde_json::Map::new();
                let mut struct_annotations = serde_json::Map::new();
                for (tag_name, annotations) in self.current_struct_annotations.iter_mut() {
                    struct_annotations
                        .insert(tag_name.clone(), json!({"value": annotations["attrs"]}));
                    if let Some(annotations_mut) = annotations.as_object_mut() {
                        if !annotations_mut.contains_key("start_wid") {
                            annotations_mut.insert("start_wid".into(), token_data["wid"].clone());
                            annotations_mut.insert("start_pos".into(), self.token_count.into());
                            struct_annotations.entry(tag_name).and_modify(|v| {
                                v.as_object_mut()
                                    .unwrap()
                                    .insert("is_start".into(), true.into());
                            });
                        }
                    }
                    struct_annotations.entry(tag_name).and_modify(|v| {
                        v.as_object_mut()
                            .unwrap()
                            .insert("start_wid".into(), annotations["start_wid"].clone());
                    });
                    let annotation_length =
                        self.token_count as i64 - annotations["start_pos"].as_i64().unwrap() + 1;
                    annotations
                        .as_object_mut()
                        .unwrap()
                        .insert("length".into(), annotation_length.into());

                    if annotations.as_object().unwrap().contains_key("attrs") {
                        for (annotation_name, v) in annotations.as_object().unwrap()["attrs"]
                            .as_object()
                            .unwrap()
                        {
                            let x = format!("{tag_name}_{annotation_name}");
                            struct_data.insert(x.clone(), v.clone());
                            self.all_word_level_annotations.insert(x);
                        }
                    }
                }

                (self.process_token)(&token_data);
                let mut all_data = BTreeMap::from_iter(&token_data);
                all_data.extend(&struct_data);

                if struct_data.contains_key("ne_name") {
                    if let Some(ne_type) = struct_data.get("ne_type").and_then(|v| v.as_str())
                        && ne_type != "MSR"
                        && ne_type != "TME"
                    {
                        self.ner_tags
                            .push(struct_data["ne_name"].as_str().unwrap().into());
                    }
                }
                if let Some(sentence_geocontext) = struct_data
                    .get("sentence__geocontext")
                    .and_then(|v| v.as_str())
                {
                    let locations: Vec<String> = sentence_geocontext
                        .split('|')
                        .map(ToString::to_string)
                        .collect();
                    let locations = &locations[1..(locations.len() - 1)];
                    self.geo_locations.extend_from_slice(&locations);
                }

                let mut str_attrs = HashMap::new();
                // all_data is sorted because of BTreeMap
                for (attr, v) in all_data {
                    let v_str = if let Some(v) = v.as_array() {
                        if v.is_empty() {
                            mappingutil::SET_DELIMITER.to_string()
                        } else {
                            format!(
                                "|{}|",
                                v.iter()
                                    .map(|v| v.as_str().unwrap())
                                    .join(mappingutil::SET_DELIMITER)
                            )
                        }
                    } else if v.is_null() {
                        mappingutil::EMPTY_SET.to_string()
                    } else {
                        v.as_str().unwrap().to_string()
                    };
                    str_attrs.insert(attr.clone(), v_str);
                }

                self.dump.last_mut().unwrap().push_str(token);
                str_attrs.insert("token".into(), token.to_string());
                self.current_part_tokens.push(str_attrs);

                let mut token_lookup_data = token_data.clone();
                token_lookup_data.extend(struct_annotations);
                self.current_token_lookup.push(TokenLookup {
                    word: token.to_string(),
                    attrs: token_lookup_data,
                    position: self.token_count as usize,
                    whitespace: None,
                });

                self.token_count += 1;

                if self.add_most_common_words && token_data["pos"].as_str().unwrap() == "NN" {
                    let mut annotation_value: Vec<String> =
                        if let Some(lemmas) = token_data.get("lemma").and_then(|v| v.as_array()) {
                            lemmas
                                .iter()
                                .filter(|lemma| {
                                    if let Some(lemma) = lemma.as_str() {
                                        !lemma.contains(":") && !lemma.contains("--")
                                    } else {
                                        false
                                    }
                                })
                                .map(|v| v.as_str().unwrap())
                                .map(ToString::to_string)
                                .collect()
                        } else {
                            if let Some(lemma_str) = self.word_attrs.get("lemma") {
                                lemma_str
                                    .split('|')
                                    .filter(|lemma| {
                                        !lemma.is_empty()
                                            && !lemma.contains(":")
                                            && !lemma.contains("--")
                                    })
                                    .map(ToString::to_string)
                                    .collect()
                            } else {
                                Vec::new()
                            }
                        };
                    if annotation_value.is_empty() {
                        annotation_value.push(token.to_string());
                    }
                    self.most_common_words.extend_from_slice(&annotation_value);
                }
            }
        }
        self.in_word = false;
        self.current_word_content.clear();

        Ok(())
    }

    fn handle_data(&mut self, data: Cow<'_, str>, tag: &[u8]) {
        let tag_str = String::from_utf8_lossy(tag);
        if self.in_word {
            self.current_word_content.push_str(data.trim());
        } else {
            if tag_str == "token" {
                let whitespaces = if let Some(tail) = self.word_attrs.get("_tail") {
                    tail.replace("\\s", " ")
                        .replace("\\n", "y")
                        .replace("\\t", "x")
                } else {
                    String::new()
                };
                let whitespaces = whitespaces.replace("x", "").replace("y", "\n");
                for ws in whitespaces.chars() {
                    self.dump.last_mut().unwrap().push(ws);
                    if self.save_whitespace_per_token {
                        if let Some(current_token_lookup) = self.current_token_lookup.last_mut() {
                            current_token_lookup.whitespace = Some(ws);
                        }
                    }
                    if ws == '\n' {
                        let current_token = self.token_count - 1;
                        self.dump.push(String::new());
                        if let Some(last_line) = self.lines.last_mut() {
                            let begin = last_line[0];
                            if begin == (current_token + 1) {
                                // last_line.clear();
                                // last_line.push(-1);
                                *last_line = vec![-1];
                            } else {
                                *last_line = vec![begin, current_token];
                            }
                        }
                        self.lines.push(vec![current_token + 1]);
                    }
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("StrixParser failed")]
struct StrixParserError;

fn iterparse_parser(
    file_name: &Path,
    strix_parser: &mut StrixParser,
) -> Result<(), Exn<IterParseError>> {
    let make_error = || IterParseError::Failure;
    let file = fs::File::open(file_name).or_raise(make_error)?;
    let reader = BufReader::new(file);
    let mut xml_reader = quick_xml::Reader::from_reader(reader);

    let mut buf = Vec::new();
    let mut tags = Vec::new();
    loop {
        match xml_reader
            .read_event_into(&mut buf)
            .or_raise(|| IterParseError::XmlError {
                path: file_name.to_path_buf(),
                pos: xml_reader.error_position(),
            })? {
            Event::Start(e) => {
                tags.push(e.name().as_ref().to_vec());
                strix_parser
                    .handle_starttag(
                        e.name().as_ref(),
                        attributes_to_map(e.attributes()).or_raise(make_error)?,
                    )
                    .or_raise(make_error)?;
            }
            Event::Text(e) => {
                let text = e.decode().or_raise(make_error)?;
                dbg!(&text);
                if let Some(tag_name) = tags.last() {
                    strix_parser.handle_data(text, tag_name);
                }
            }
            Event::End(e) => {
                strix_parser
                    .handle_endtag(e.name().as_ref())
                    .or_raise(make_error)?;
            }
            _ => {}
        }
    }
}

fn attributes_to_map(
    attributes: Attributes,
) -> Result<HashMap<String, String>, Exn<AttributeError>> {
    let mut attrs = HashMap::new();

    for attribute in attributes {
        let attribute = attribute.or_raise(|| AttributeError)?;
        let name =
            String::from_utf8(attribute.key.as_ref().to_vec()).or_raise(|| AttributeError)?;
        let value = String::from_utf8(attribute.value.to_vec()).or_raise(|| AttributeError)?;
        if let Some(old_value) = attrs.insert(name, value) {
            log::warn!(
                "attribute '{}' already had a value. Keeping the new value. old_value={}, new_value={}",
                String::from_utf8_lossy(attribute.key.as_ref()),
                old_value,
                String::from_utf8_lossy(attribute.value.as_ref())
            );
        }
    }
    Ok(attrs)
}

#[derive(Debug, thiserror::Error)]
#[error("attribute error")]
struct AttributeError;

#[derive(Debug, thiserror::Error)]
pub enum IterParseError {
    #[error("iterparse_parser failed")]
    Failure,
    #[error("xml error at pos '{pos}' in file '{path}'")]
    XmlError { path: PathBuf, pos: u64 },
}
