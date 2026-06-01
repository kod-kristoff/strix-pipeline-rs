use std::collections::HashSet;

use capitalize::Capitalize;
use exn::{Exn, ResultExt};
use hashbrown::HashMap;
use serde_json::{Map, Value, json};
use time::UtcDateTime;

use crate::{StrixConfig, shared};

#[derive(Debug, thiserror::Error)]
#[error("Failed to decode sparv config")]
pub struct DecodeSparvError;

/// py: main(corpus_name) && createConfig(data)
pub fn convert_to_strix_config(
    corpus_name: &str,
    config: &StrixConfig,
) -> Result<CorpusConfig, Exn<DecodeSparvError>> {
    let make_error = || DecodeSparvError;
    let sparv2strix_path = config
        .settings_dir()
        .join(format!("sparv2strix/{}.yaml", corpus_name));
    dbg!(&sparv2strix_path);
    let data: Value = shared::load_yaml_from_path(&sparv2strix_path).or_raise(make_error)?;
    dbg!(&data);
    let mut corpus_data = convert_config(&data, config).or_raise(make_error)?;
    dbg!(&corpus_data);

    if corpus_data.title.is_empty() {
        corpus_data.title = "n/a".into();
    }
    if corpus_data.document_id.is_empty() {
        corpus_data.document_id = "generated".into();
    }

    corpus_data.updated_at = UtcDateTime::now().unix_timestamp();

    let corpus_path = config
        .settings_dir()
        .join(format!("corpora/{}.yaml", &corpus_data.corpus_id));
    println!("writing config to '{}'", corpus_path.display());
    shared::dump_yaml_from_path(&corpus_data, &corpus_path).or_raise(make_error)?;
    Ok(corpus_data)
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CorpusConfig {
    analyze_config: AnalyzeConfig,
    text_tags: Vec<String>,
    mode_id: String,
    mode_name: HashMap<String, String>,
    folder_name: String,
    split: String,
    corpus_name: LangValue,
    corpus_description: LangValue,
    title: String,
    document_id: String,
    updated_at: i64,
    corpus_id: String,
    extra: HashMap<String, Value>,
}

impl CorpusConfig {
    pub fn corpus_dir(&self) -> Option<&str> {
        None
    }
    pub fn corpus_id(&self) -> &str {
        &self.corpus_id
    }
    pub fn split(&self) -> &str {
        &self.split
    }
    pub fn text_tags(&self) -> &Vec<String> {
        &self.text_tags
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct LangValue {
    #[serde(flatten)]
    values: HashMap<String, String>,
}

impl LangValue {
    pub fn add(&mut self, lang: String, value: String) {
        self.values.insert(lang, value);
    }
}

impl From<&Value> for LangValue {
    fn from(value: &Value) -> Self {
        let mut out = Self::default();
        if let Some(obj) = value.as_object() {
            for (lang, value) in obj {
                out.add(lang.into(), value.as_str().unwrap().into());
            }
        }
        if let Some(v) = out.values.get("swe")
            && !out.values.contains_key("eng")
        {
            out.add("eng".into(), v.clone());
        } else if let Some(v) = out.values.get("eng")
            && !out.values.contains_key("swe")
        {
            out.add("swe".into(), v.clone());
        }
        out
    }
}
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AnalyzeConfig {
    struct_attributes: HashMap<String, Value>,
    word_attributes: Vec<HashMap<String, Value>>,
    text_attributes: Vec<HashMap<String, Value>>,
}

#[derive(Debug, thiserror::Error)]
#[error("Converting config failed.")]
struct ConvertConfigError;

/// py: getConfig(data)
fn convert_config(
    data: &Value,
    config: &StrixConfig,
) -> Result<CorpusConfig, Exn<ConvertConfigError>> {
    let make_error = || ConvertConfigError;

    let mut corpus_template = CorpusConfig::default();

    let mut year_exist = false;
    let mut text_list: Vec<HashMap<String, Value>> = Vec::new();
    let mut text_list_x: Vec<String> = Vec::new();
    let mut temp_list_struct: Vec<HashMap<String, Value>> = Vec::new();

    for (key, value) in data.as_object().unwrap().iter() {
        if key == "text_attributes" {
            for x in value.as_array().unwrap() {
                for (k1, v1) in x.as_object().unwrap() {
                    if k1 == "text:year" {
                        year_exist = true;
                    }
                    let k1_parts: Vec<&str> = k1.split(':').collect();
                    if let Some(v1_str) = v1.as_str() {
                        text_list.push(make_map(k1_parts[1], v1_str.replace("text_", "").into()));
                        text_list_x.push(k1_parts[1].to_string());
                    } else {
                        text_list.push(make_map(k1_parts[1], replace_key(v1, k1_parts[1])));
                        text_list_x.push(k1_parts[1].to_string());
                    }
                }
            }
        } else if key == "struct_attributes" {
            let mut struct_attributes = HashMap::new();
            let struct_keys_path = config.settings_dir().join("attributes/struct_elems.yaml");

            let struct_keys: Map<String, Value> =
                shared::load_yaml_from_path(&struct_keys_path).or_raise(make_error)?;

            dbg!(&struct_keys);
            let (mut text_xtra, struct_modified, text_attr) = restructure(value, &struct_keys);
            text_list_x.extend_from_slice(text_attr.as_slice());

            for (key1, value1) in struct_modified {
                if struct_keys.contains_key(&key1) {
                    let mut temp_dict1 = Vec::new();
                    for x in value1.as_array().unwrap() {
                        for (k1, v1) in x.as_object().unwrap() {
                            if v1.is_string() {
                                temp_dict1.push(json!({k1: v1}));
                            } else {
                                temp_dict1.push(json!({k1: replace_key_struct(v1, k1)}));
                            }
                        }
                    }
                    struct_attributes.insert(key1, temp_dict1.into());
                } else {
                    for x in value1.as_array().unwrap() {
                        for (k1, v1) in x.as_object().unwrap() {
                            if v1.is_string() {
                                temp_list_struct.push(make_map(k1, create_dict(k1)));
                            } else {
                                temp_list_struct.push(make_map(k1, replace_key(v1, k1)));
                            }
                        }
                    }
                }
            }

            text_xtra.push("text".into());
            corpus_template.text_tags = text_xtra;
            corpus_template.analyze_config.struct_attributes = struct_attributes;
        } else if key == "word_attributes" {
            let mut word_list = vec![make_map("lemgram", "lemgram".into())];
            for x in value.as_array().unwrap() {
                for (k1, v1) in x.as_object().unwrap() {
                    if !["ufeats", "lex", "_tail", "_head"].contains(&k1.as_str()) {
                        if v1.is_string() {
                            word_list.push(make_map(k1, v1.to_owned()));
                        } else {
                            word_list.push(make_map(k1, replace_key(v1, k1)));
                        }
                    }
                }
            }
            corpus_template.analyze_config.word_attributes = word_list;
        } else if key == "mode" {
            let value_name = value.as_array().unwrap()[0].as_object().unwrap()["name"]
                .as_str()
                .unwrap();
            corpus_template.mode_id = value_name.into();
            corpus_template.mode_name = config
                .corpusconf()
                .get_mode(value_name)
                .or_raise(make_error)?
                .translation_name
                .clone();
            corpus_template.folder_name = "".into();
        } else if key == "text_annotation" {
            corpus_template.split = value.as_str().unwrap().to_owned();
        } else if key == "corpus_name" {
            corpus_template.corpus_name = value.into();
        } else if key == "corpus_description" {
            corpus_template.corpus_description = value.into();
        } else if key == "corpus_id" {
            corpus_template.corpus_id = value.as_str().unwrap().into();
        } else {
            corpus_template.extra.insert(key.into(), value.to_owned());
        }
    }
    text_list.extend_from_slice(temp_list_struct.as_slice());
    if !year_exist {
        text_list.push(make_map("year", "year".into()));
    }
    corpus_template.analyze_config.text_attributes = text_list;
    for x_item in text_list_x {
        if x_item == "_id" {
            corpus_template.document_id = x_item;
        } else if x_item == "title"
            || x_item == "titel"
            || x_item.contains("title")
            || x_item.contains("name")
        {
            corpus_template.title = x_item;
        }
    }
    Ok(corpus_template)
}

fn make_map<S: Into<String>, T>(s: S, t: T) -> HashMap<String, T> {
    let mut map = HashMap::new();
    map.insert(s.into(), t);
    map
}
fn create_dict(item_name: &str) -> Value {
    json!({
        "translation_name": {
            "swe": item_name.replace('_', " ").capitalize(),
            "eng": item_name.replace('_', " ").capitalize(),
        },
        "name": item_name,
    })
}
fn replace_key(item: &Value, item_name: &str) -> Value {
    let mut item_x = json!({});
    if let Some(item_obj) = item.as_object() {
        for (key, value) in item_obj {
            if let Some(value_str) = value.as_str()
                && (key == "label" || key == "preset")
            {
                item_x.as_object_mut().unwrap().insert(
                    "translation_name".into(),
                    json!({
                        "swe": value_str.replace('_', " ").capitalize(),
                        "eng": value_str.replace('_', " ").capitalize(),
                    }),
                );
            } else if let Some(value_dict) = value.as_object()
                && (key == "label" || key == "preset")
            {
                item_x.as_object_mut().unwrap().insert(
                    "translation_name".into(),
                    json!({
                        "swe": value_dict["swe"].as_str().unwrap().capitalize(),
                        "eng": value_dict["eng"].as_str().unwrap().capitalize(),
                    }),
                );
            } else {
                item_x
                    .as_object_mut()
                    .unwrap()
                    .insert(key.into(), value.to_owned());
            }
        }
    }
    item_x
        .as_object_mut()
        .unwrap()
        .insert("name".into(), item_name.into());
    item_x
}

fn replace_key_struct(item: &Value, item_name: &str) -> Value {
    let item_name_parts: Vec<&str> = item_name.split('_').collect();
    replace_key(item, item_name_parts[1])
}

fn restructure(
    data: &Value,
    struct_keys: &Map<String, Value>,
) -> (Vec<String>, Map<String, Value>, Vec<String>) {
    let mut re_create = Map::new();
    let mut text_elements = HashSet::new();
    let mut text_attr = HashSet::new();
    if let Some(data_arr) = data.as_array() {
        for item in data_arr {
            if let Some(item_obj) = item.as_object() {
                for (item_key, item_value) in item_obj {
                    if item_key.contains(':') {
                        let item_key_parts: Vec<&str> = item_key.split(':').collect();
                        if re_create.contains_key(item_key_parts[0]) {
                            re_create
                                .get_mut(item_key_parts[0])
                                .unwrap()
                                .as_array_mut()
                                .unwrap()
                                .push(json!({item_key.replace(':',"_"): item_value.to_owned()}));
                        } else {
                            re_create.insert(
                                item_key_parts[0].into(),
                                json!([{item_key.replace(':',"_"): item_value.to_owned()}]),
                            );
                        }
                        if !struct_keys.contains_key(item_key_parts[0]) {
                            text_elements.insert(item_key_parts[0].into());
                            text_attr.insert(item_key_parts[0].into());
                        }
                    } else {
                        re_create.insert(item_key.into(), item_value.to_owned());
                    }
                }
            }
        }
    }
    (
        text_elements.into_iter().collect(),
        re_create,
        text_attr.into_iter().collect(),
    )
}
