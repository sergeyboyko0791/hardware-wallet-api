use std::path::Path;

const MESSAGES_PROTO: &str = "protos/messages.proto";
const MESSAGES_COMMON_PROTO: &str = "protos/messages-common.proto";
const MESSAGES_MANAGEMENT_PROTO: &str = "protos/messages-management.proto";

const MESSAGES_BITCOIN_PROTO: &str = "protos/messages-bitcoin.proto";
const MESSAGES_TEZOS_PROTO: &str = "protos/messages-tezos.proto";

use protoc_rust::Customize;

fn main() {
    // prost_build::compile_protos(
    //     &[MESSAGES_PROTO, MESSAGES_COMMON_PROTO, MESSAGES_MANAGEMENT_PROTO, MESSAGES_BITCOIN_PROTO, MESSAGES_TEZOS_PROTO],
    //     &["protos"],
    // ).unwrap();

    protoc_rust::Codegen::new()
        .out_dir("src/protos")
        .inputs(&[MESSAGES_PROTO, MESSAGES_COMMON_PROTO, MESSAGES_MANAGEMENT_PROTO, MESSAGES_BITCOIN_PROTO, MESSAGES_TEZOS_PROTO])
        .include("protos")
        .run()
        .expect("protoc");
}
