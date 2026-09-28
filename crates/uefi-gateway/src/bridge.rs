use tonic::Status;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use uefi_proto::descriptor::{DynamicMessage, MessageDescriptor};

/// Кодек DynamicMessage↔bytes: штатный ProstCodec требует Decode: Default,
/// DynamicMessage не реализует Default (дескриптор не статичен).
pub struct DynCodec {
    output: MessageDescriptor,
}

impl DynCodec {
    pub fn new(output: MessageDescriptor) -> Self {
        Self { output }
    }
}

impl Codec for DynCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynEncoder;
    type Decoder = DynDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        DynEncoder
    }

    fn decoder(&mut self) -> Self::Decoder {
        DynDecoder {
            output: self.output.clone(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct DynEncoder;

impl Encoder for DynEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(&mut self, item: DynamicMessage, buf: &mut EncodeBuf<'_>) -> Result<(), Status> {
        use prost::Message as _;
        item.encode(buf)
            .map_err(|e| Status::internal(format!("encode: {e}")))
    }
}

#[derive(Clone)]
pub struct DynDecoder {
    output: MessageDescriptor,
}

impl Decoder for DynDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(&mut self, buf: &mut DecodeBuf<'_>) -> Result<Option<DynamicMessage>, Status> {
        DynamicMessage::decode(self.output.clone(), buf)
            .map(Some)
            .map_err(|e| Status::internal(format!("decode: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uefi_proto::descriptor::pool;

    #[test]
    fn json_request_scalar_fields() {
        let desc = pool()
            .get_message_by_name("engine.ImageNodesListRequest")
            .unwrap();
        let dm =
            DynamicMessage::deserialize(desc, json!({"imageId": "i-1", "filter": "fv"})).unwrap();
        assert_eq!(
            dm.get_field_by_name("image_id").unwrap().as_str(),
            Some("i-1")
        );
        assert_eq!(dm.get_field_by_name("filter").unwrap().as_str(), Some("fv"));
    }

    #[test]
    fn json_bytes_base64_roundtrip() {
        let desc = pool().get_message_by_name("engine.NvarVarInfo").unwrap();
        let dm = DynamicMessage::deserialize(desc, json!({"name": "S", "data": "AQID"})).unwrap();
        let out = serde_json::to_value(&dm).unwrap();
        assert_eq!(out["data"].as_str().unwrap(), "AQID");
        let dm2 = DynamicMessage::deserialize(
            pool().get_message_by_name("engine.NvarVarInfo").unwrap(),
            out,
        )
        .unwrap();
        assert_eq!(dm, dm2);
    }

    #[test]
    fn json_enum_by_name() {
        let desc = pool()
            .get_message_by_name("engine.ImageNodeInsertRequest")
            .unwrap();
        let dm = DynamicMessage::deserialize(desc, json!({"mode": "AFTER"})).unwrap();
        let out = serde_json::to_value(&dm).unwrap();
        assert!(
            out["mode"]
                .as_str()
                .map(|s| s == "AFTER" || s == "2")
                .unwrap_or(false)
                || out["mode"].as_i64() == Some(2)
        );
    }

    #[test]
    fn json_bad_type_is_error() {
        let desc = pool()
            .get_message_by_name("engine.ImageOpenRequest")
            .unwrap();
        assert!(DynamicMessage::deserialize(desc, json!({"path": 123})).is_err());
    }

    #[test]
    fn json_uint64_accepts_number() {
        let desc = pool().get_message_by_name("engine.NvarVarInfo").unwrap();
        let dm = DynamicMessage::deserialize(desc, json!({"offset": 16})).unwrap();
        assert_eq!(dm.get_field_by_name("offset").unwrap().as_u64(), Some(16));
    }
}
