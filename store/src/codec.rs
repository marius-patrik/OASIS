//! Lossless, tagged encoding for native contract values. Arbitrary source
//! engines may define new value types without introducing SQL columns.
use oasis_contracts::{Id, Revision, Snapshot, TypeRef, TypedValue, Value};
use serde_json::{json, Map, Number, Value as Json};
use super::{invalid, StoreResult};

fn number(v: f64) -> StoreResult<Json> {
    Ok(Json::Number(Number::from_f64(v).ok_or_else(|| invalid("nonfinite native float"))?))
}
pub fn encode_value(value: &Value) -> StoreResult<Json> {
    Ok(match value {
        Value::Null => json!({"k":"null"}),
        Value::Bool(v) => json!({"k":"bool","v":v}),
        Value::Int(v) => json!({"k":"int","v":v}),
        Value::UInt(v) => json!({"k":"uint","v":v}),
        Value::Float(v) => json!({"k":"float","v":number(*v)?}),
        Value::String(v) => json!({"k":"string","v":v}),
        Value::Bytes(v) => json!({"k":"bytes","v":v}),
        Value::Ref(v) => json!({"k":"ref","v":v.0.to_string()}),
        Value::Sequence(v) => Json::Object(Map::from_iter([
            ("k".into(),Json::String("sequence".into())),
            ("v".into(),Json::Array(v.iter().map(encode_value).collect::<StoreResult<_>>()?)),
        ])),
        Value::Map(v) => Json::Object(Map::from_iter([
            ("k".into(),Json::String("map".into())),
            ("v".into(),Json::Object(v.iter()
                .map(|(k,v)|Ok((k.clone(),encode_value(v)?)))
                .collect::<StoreResult<_>>()?)),
        ])),
    })
}

fn payload(json: &Json) -> StoreResult<&Json> {
    json.get("v").ok_or_else(|| invalid("native value missing payload"))
}
pub fn decode_value(json: &Json) -> StoreResult<Value> {
    let kind=json.get("k").and_then(Json::as_str)
        .ok_or_else(||invalid("native value missing tag"))?;
    Ok(match kind {
        "null" => Value::Null,
        "bool" => Value::Bool(payload(json)?.as_bool().ok_or_else(||invalid("bad boolean"))?),
        "int" => Value::Int(payload(json)?.as_i64().ok_or_else(||invalid("bad integer"))?),
        "uint" => Value::UInt(payload(json)?.as_u64().ok_or_else(||invalid("bad uint"))?),
        "float" => Value::Float(payload(json)?.as_f64().ok_or_else(||invalid("bad float"))?),
        "string" => Value::String(payload(json)?.as_str()
            .ok_or_else(||invalid("bad string"))?.into()),
        "ref" => Value::Ref(Id(payload(json)?.as_str()
            .ok_or_else(||invalid("bad ref"))?.parse::<u128>()?)),
        "bytes" => Value::Bytes(payload(json)?.as_array()
            .ok_or_else(||invalid("bad bytes"))?.iter()
            .map(|n| {
                let value=n.as_u64().ok_or_else(||invalid("bad byte"))?;
                Ok(u8::try_from(value)?)
            }).collect::<StoreResult<_>>()?),
        "sequence" => Value::Sequence(payload(json)?.as_array()
            .ok_or_else(||invalid("bad sequence"))?
            .iter().map(decode_value).collect::<StoreResult<_>>()?),
        "map" => Value::Map(payload(json)?.as_object()
            .ok_or_else(||invalid("bad map"))?
            .iter().map(|(key,v)|Ok((key.clone(),decode_value(v)?)))
            .collect::<StoreResult<_>>()?),
        _ => return Err(invalid("unknown native value tag")),
    })
}

pub fn encode_snapshot(snapshot: &Snapshot) -> StoreResult<Json> {
    let mut state=Vec::with_capacity(snapshot.state.len());
    for entry in &snapshot.state {
        state.push(json!({
            "namespace":entry.type_ref.namespace,
            "name":entry.type_ref.name,
            "version":entry.type_ref.version,
            "value":encode_value(&entry.data)?,
        }));
    }
    Ok(json!({
        "native_revision":snapshot.revision.0,
        "state":state,
        "binary_artifact":snapshot.binary_artifact.map(|id|id.0.to_string()),
    }))
}
pub fn decode_snapshot(entity_id: Id, doc: &Json) -> StoreResult<Snapshot> {
    let revision=doc.get("native_revision").and_then(Json::as_u64)
        .ok_or_else(|| invalid("native revision missing"))?;
    let state=doc.get("state").and_then(Json::as_array)
        .ok_or_else(|| invalid("native components missing"))?
        .iter().map(|value| {
            let text=|key|->StoreResult<String>{
                Ok(value.get(key).and_then(Json::as_str)
                    .ok_or_else(||invalid("invalid component name"))?.to_string())
            };
            let version=value.get("version").and_then(Json::as_u64)
                .ok_or_else(||invalid("component version missing"))?;
            Ok(TypedValue{
                type_ref: TypeRef{
                    namespace:text("namespace")?,
                    name:text("name")?,
                    version:u32::try_from(version)?,
                },
                data:decode_value(value.get("value")
                    .ok_or_else(||invalid("component payload missing"))?)?,
            })
        }).collect::<StoreResult<Vec<_>>>()?;
    let binary_artifact=doc.get("binary_artifact")
        .and_then(Json::as_str)
        .map(|s|s.parse::<u128>().map(Id))
        .transpose()?;
    Ok(Snapshot{entity_id,state,revision:Revision(revision),binary_artifact})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn native_snapshot_roundtrip_without_losing_types() {
        let cases=vec![
            Value::Null, Value::Bool(true),Value::Int(-5),
            Value::UInt(u64::MAX),Value::Float(-1.25),
            Value::String("source-game".into()),Value::Ref(Id(u128::MAX)),
            Value::Bytes(vec![0,1,255]),
            Value::Sequence(vec![Value::Int(-1),Value::UInt(1)]),
            Value::Map(BTreeMap::from([("x".into(),Value::Bool(false))])),
        ];
        let s=Snapshot{
            entity_id:Id(20),revision:Revision(12),binary_artifact:Some(Id(50)),
            state:cases.into_iter().enumerate().map(|(i,v)| TypedValue {
                type_ref:TypeRef{namespace:"game".into(),name:format!("custom{i}"),version:1},
                data:v,
            }).collect(),
        };
        let json=encode_snapshot(&s).unwrap();
        let loaded=decode_snapshot(s.entity_id,&json).unwrap();
        assert_eq!(loaded.revision,s.revision);
        assert_eq!(loaded.binary_artifact,s.binary_artifact);
        assert_eq!(loaded.state,s.state);
    }
    #[test]
    fn canonical_component_encoding_keeps_binary_unsigned_references_and_literal_keys() {
        let input=Value::Map(BTreeMap::from([
            ("k".into(),Value::String("ordinary-key".into())),
            ("v".into(),Value::Int(-1)),
            ("binary".into(),Value::Bytes(vec![0,1,254,255])),
            ("large".into(),Value::UInt(u64::MAX)),
            ("ref".into(),Value::Ref(Id(u128::MAX))),
        ]));
        let encoded=encode_value(&input).unwrap();
        let decoded=decode_value(&encoded).unwrap();
        assert_eq!(decoded,input);
        assert!(decode_value(&json!({"k":"bytes","v":[256]})).is_err());
    }

    #[test]
    fn rejects_unrepresentable_native_values() {
        assert!(encode_value(&Value::Float(f64::NAN)).is_err());
        assert!(decode_value(&json!({"k":"unknown"})).is_err());
    }
}
