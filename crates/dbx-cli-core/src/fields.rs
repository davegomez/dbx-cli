use serde_json::{Map, Value};

pub fn select_fields(value: &Value, fields: &str) -> Value {
    let mut out = Value::Object(Map::new());
    for raw_path in fields.split(',') {
        let parts: Vec<&str> = raw_path
            .trim()
            .split('.')
            .filter(|part| !part.is_empty())
            .collect();
        if !parts.is_empty() {
            select_into(value, &mut out, &parts);
        }
    }
    out
}

fn select_into(src: &Value, dst: &mut Value, parts: &[&str]) -> bool {
    if parts.is_empty() {
        *dst = src.clone();
        return true;
    }

    match src {
        Value::Object(src_map) => {
            let Some(next_src) = src_map.get(parts[0]) else {
                return false;
            };

            if parts.len() == 1 {
                ensure_object(dst).insert(parts[0].to_string(), next_src.clone());
                return true;
            }

            let mut next_dst = match next_src {
                Value::Array(items) => Value::Array(vec![Value::Object(Map::new()); items.len()]),
                _ => Value::Object(Map::new()),
            };
            if !select_into(next_src, &mut next_dst, &parts[1..]) {
                return false;
            }
            ensure_object(dst).insert(parts[0].to_string(), next_dst);
            true
        }
        Value::Array(items) => {
            let mut selected_any = false;
            let mut selected_items = Vec::with_capacity(items.len());
            for item in items {
                let mut next_dst = Value::Object(Map::new());
                let selected = select_into(item, &mut next_dst, parts);
                selected_any |= selected;
                selected_items.push(next_dst);
            }
            if selected_any {
                *dst = Value::Array(selected_items);
            }
            selected_any
        }
        _ => false,
    }
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !matches!(value, Value::Object(_)) {
        *value = Value::Object(Map::new());
    }
    match value {
        Value::Object(map) => map,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selects_top_level_and_nested_fields() {
        let value = json!({
            "entries": [{"name": "a", "id": "id:a", "size": 1}],
            "cursor": "c",
            "has_more": false
        });
        let selected = select_fields(&value, "entries.name,cursor");
        assert_eq!(selected, json!({"entries": [{"name": "a"}], "cursor": "c"}));
    }

    #[test]
    fn trims_whitespace_and_ignores_empty_paths() {
        let value = json!({"cursor": "c", "has_more": false});
        let selected = select_fields(&value, " cursor, ,has_more,,");
        assert_eq!(selected, json!({"cursor": "c", "has_more": false}));
    }

    #[test]
    fn omits_missing_fields() {
        let value = json!({"cursor": "c"});
        let selected = select_fields(&value, "missing,cursor.nested");
        assert_eq!(selected, json!({}));
    }

    #[test]
    fn can_select_scalar_root_when_path_is_consumed() {
        let value = json!({"metadata": "value"});
        let selected = select_fields(&value, "metadata");
        assert_eq!(selected, json!({"metadata": "value"}));
    }
}
