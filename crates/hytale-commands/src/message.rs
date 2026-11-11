//! Formatted chat messages and the Java `Message.parse` JSON subset.
//!
//! Java chat formatting is event-driven and ultimately writes a
//! [`FormattedMessage`]: a translation id plus parameters, raw text, styled
//! children, a colour or a link. Command output stays plain [`String`] text;
//! the chat relay and `/say` use these helpers to preserve translation ids and
//! styling instead of flattening every line to raw text.

use std::collections::BTreeMap;

use hytale_protocol::generated::common::{
    BoolParamValue, DoubleParamValue, FormattedMessage, FormattedMessageImage, IntParamValue,
    LongParamValue, ParamValue, StringParamValue,
};
use serde_json::Value;

/// A plain raw-text message.
#[must_use]
pub fn raw_message(text: impl Into<String>) -> FormattedMessage {
    FormattedMessage {
        raw_text: Some(text.into()),
        ..FormattedMessage::default()
    }
}

/// A translated message with a raw-text fallback.
///
/// Java sends only the translation id and lets the client resolve it. The
/// fallback keeps tooling and logs readable and is harmless when the client
/// resolves `message_id`.
#[must_use]
pub fn translated_message(
    message_id: impl Into<String>,
    params: &[(&str, &str)],
    fallback: impl Into<String>,
) -> FormattedMessage {
    let mut message = raw_message(fallback);
    message.message_id = Some(message_id.into());
    if !params.is_empty() {
        message.params = Some(
            params
                .iter()
                .map(|(key, value)| {
                    (
                        (*key).to_string(),
                        ParamValue::StringParamValue(StringParamValue {
                            value: Some((*value).to_string()),
                        }),
                    )
                })
                .collect(),
        );
    }
    message
}

/// Apply a Java-style colour name or `#rrggbb` value.
#[must_use]
pub fn with_color(mut message: FormattedMessage, color: impl Into<String>) -> FormattedMessage {
    message.color = Some(color.into());
    message
}

/// Parse the Java `Message` JSON form used by `/say { ... }`.
///
/// The codec accepts the uppercase keys (`RawText`, `MessageId`, `Params`,
/// `Children`, ...). Lowercase/camel-case spellings are accepted too so
/// hand-written command payloads are less brittle.
pub fn parse_message(input: &str) -> Result<FormattedMessage, String> {
    let value: Value =
        serde_json::from_str(input).map_err(|error| format!("invalid JSON: {error}"))?;
    parse_value(&value)
}

fn parse_value(value: &Value) -> Result<FormattedMessage, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "a message must be a JSON object".to_string())?;
    Ok(FormattedMessage {
        raw_text: string_field(object, &["RawText", "rawText", "raw_text"])?,
        message_id: string_field(object, &["MessageId", "messageId", "message_id"])?,
        color: string_field(object, &["Color", "color"])?,
        link: string_field(object, &["Link", "link"])?,
        bold: bool_field(object, &["Bold", "bold"])?,
        italic: bool_field(object, &["Italic", "italic"])?,
        monospace: bool_field(object, &["Monospace", "monospace"])?,
        underlined: bool_field(
            object,
            &["Underline", "Underlined", "underline", "underlined"],
        )?,
        strikethrough: bool_field(object, &["Strikethrough", "strikethrough"])?,
        markup_enabled: bool_field(object, &["MarkupEnabled", "markupEnabled"])?.unwrap_or(false),
        params: params_field(object, &["Params", "params"])?,
        message_params: message_params_field(object, &["MessageParams", "messageParams"])?,
        children: children_field(object, &["Children", "children"])?,
        image: image_field(object, &["Image", "image"])?,
    })
}

fn field<'a>(object: &'a serde_json::Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| object.get(*name))
}

fn string_field(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Result<Option<String>, String> {
    match field(object, names) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("`{}` must be a string", names[0])),
    }
}

fn bool_field(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Result<Option<bool>, String> {
    match field(object, names) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("`{}` must be a boolean", names[0])),
    }
}

fn params_field(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Result<Option<BTreeMap<String, ParamValue>>, String> {
    let Some(value) = field(object, names) else {
        return Ok(None);
    };
    let Some(map) = value.as_object() else {
        return Err(format!("`{}` must be an object", names[0]));
    };
    let mut params = BTreeMap::new();
    for (key, value) in map {
        params.insert(key.clone(), parse_param(value)?);
    }
    Ok(Some(params))
}

fn parse_param(value: &Value) -> Result<ParamValue, String> {
    match value {
        Value::String(value) => Ok(ParamValue::StringParamValue(StringParamValue {
            value: Some(value.clone()),
        })),
        Value::Bool(value) => Ok(ParamValue::BoolParamValue(BoolParamValue { value: *value })),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                return Ok(match i32::try_from(value) {
                    Ok(value) => ParamValue::IntParamValue(IntParamValue { value }),
                    Err(_) => ParamValue::LongParamValue(LongParamValue { value }),
                });
            }
            value
                .as_f64()
                .map(|value| ParamValue::DoubleParamValue(DoubleParamValue { value }))
                .ok_or_else(|| "unsupported number parameter".to_string())
        }
        _ => Err("message parameters must be strings, booleans or numbers".to_string()),
    }
}

fn message_params_field(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Result<Option<BTreeMap<String, FormattedMessage>>, String> {
    let Some(value) = field(object, names) else {
        return Ok(None);
    };
    let Some(map) = value.as_object() else {
        return Err(format!("`{}` must be an object", names[0]));
    };
    let mut params = BTreeMap::new();
    for (key, value) in map {
        params.insert(key.clone(), parse_value(value)?);
    }
    Ok(Some(params))
}

fn children_field(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Result<Option<Vec<FormattedMessage>>, String> {
    let Some(value) = field(object, names) else {
        return Ok(None);
    };
    let Some(children) = value.as_array() else {
        return Err(format!("`{}` must be an array", names[0]));
    };
    children
        .iter()
        .map(parse_value)
        .collect::<Result<_, _>>()
        .map(Some)
}

fn image_field(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Result<Option<FormattedMessageImage>, String> {
    let Some(value) = field(object, names) else {
        return Ok(None);
    };
    let Some(image) = value.as_object() else {
        return Err(format!("`{}` must be an object", names[0]));
    };
    let file_path = string_field(image, &["FilePath", "filePath", "file_path"])?
        .ok_or_else(|| "message image needs `FilePath`".to_string())?;
    let width = integer_field(image, &["Width", "width"])?.unwrap_or(0);
    let height = integer_field(image, &["Height", "height"])?.unwrap_or(0);
    Ok(Some(FormattedMessageImage {
        file_path,
        width,
        height,
    }))
}

fn integer_field(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Result<Option<i32>, String> {
    match field(object, names) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => value
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| format!("`{}` must fit in an i32", names[0])),
        Some(_) => Err(format!("`{}` must be an integer", names[0])),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_message, translated_message};
    use hytale_protocol::generated::common::ParamValue;

    #[test]
    fn translation_messages_carry_id_params_and_fallback() {
        let message = translated_message(
            "server.chat.playerMessage",
            &[("username", "Alice"), ("message", "hi")],
            "Alice: hi",
        );
        assert_eq!(
            message.message_id.as_deref(),
            Some("server.chat.playerMessage")
        );
        assert_eq!(message.raw_text.as_deref(), Some("Alice: hi"));
        let params = message.params.expect("params");
        assert!(matches!(
            params.get("username"),
            Some(ParamValue::StringParamValue(value))
                if value.value.as_deref() == Some("Alice")
        ));
    }

    #[test]
    fn parses_java_message_json_with_styles_and_children() {
        let message = parse_message(
            r##"{
                "MessageId": "example.title",
                "Color": "#00ffff",
                "Bold": true,
                "Params": {"count": 3, "ready": true, "ratio": 1.5},
                "Children": [{"RawText": "!"}]
            }"##,
        )
        .expect("message");
        assert_eq!(message.message_id.as_deref(), Some("example.title"));
        assert_eq!(message.color.as_deref(), Some("#00ffff"));
        assert_eq!(message.bold, Some(true));
        assert_eq!(message.children.expect("children").len(), 1);
        let params = message.params.expect("params");
        assert!(matches!(
            params.get("count"),
            Some(ParamValue::IntParamValue(value)) if value.value == 3
        ));
        assert!(matches!(
            params.get("ready"),
            Some(ParamValue::BoolParamValue(value)) if value.value
        ));
        assert!(matches!(
            params.get("ratio"),
            Some(ParamValue::DoubleParamValue(value)) if (value.value - 1.5).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn rejects_non_object_and_bad_style_values() {
        assert!(parse_message("[]").is_err());
        assert!(parse_message(r#"{"Bold":"yes"}"#).is_err());
    }
}
