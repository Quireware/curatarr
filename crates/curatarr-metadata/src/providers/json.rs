use serde_json::Value;

pub fn get_str(v: &Value, key: &str) -> Option<String> {
    as_nonempty(v.get(key))
}

pub fn as_nonempty(v: Option<&Value>) -> Option<String> {
    v.and_then(|x| {
        if let Some(s) = x.as_str() {
            nonempty(s)
        } else {
            x.get("value").and_then(Value::as_str).and_then(nonempty)
        }
    })
}

pub fn nonempty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

pub fn get_i64(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_i64)
}

pub fn get_f64(v: &Value, key: &str) -> Option<f64> {
    v.get(key)
        .and_then(Value::as_f64)
        .or_else(|| get_i64(v, key).map(i64_to_f64))
}

fn i64_to_f64(v: i64) -> f64 {
    f64::from(i32::try_from(v).unwrap_or(0))
}

pub fn get_u32(v: &Value, key: &str) -> Option<u32> {
    get_i64(v, key).and_then(|n| u32::try_from(n).ok())
}

pub fn str_list(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().and_then(nonempty))
                .collect()
        })
        .unwrap_or_default()
}

pub fn first_str_in(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| get_str(v, k))
}
