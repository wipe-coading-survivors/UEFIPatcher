pub mod engine {
    tonic::include_proto!("engine");
}
pub use engine::*;

pub mod descriptor {
    pub use prost_reflect::{
        DescriptorPool, DynamicMessage, MessageDescriptor, MethodDescriptor, ServiceDescriptor,
    };
    use std::sync::OnceLock;

    pub const SERVICE_NAME: &str = "engine.EngineService";

    static BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/engine.binDescriptor"));
    static POOL: OnceLock<DescriptorPool> = OnceLock::new();

    /// Пул дескрипторов engine.proto; паникует на невалидном блобе (блоб генерится build.rs).
    pub fn pool() -> &'static DescriptorPool {
        POOL.get_or_init(|| DescriptorPool::decode(BYTES).expect("engine.binDescriptor is valid"))
    }

    pub fn service() -> ServiceDescriptor {
        pool()
            .get_service_by_name(SERVICE_NAME)
            .expect("engine.EngineService in descriptor")
    }

    pub fn method(name: &str) -> Option<MethodDescriptor> {
        service().methods().find(|m| m.name() == name)
    }

    pub fn method_names() -> Vec<String> {
        service().methods().map(|m| m.name().to_string()).collect()
    }
}
