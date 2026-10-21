//! Connection IR tests against the real generated family and a bad fixture.

use std::path::{Path, PathBuf};

use protocol_dumper::connection;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn real_connection_dir() -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../.hytale-source/hytale-shared-source/HytaleServer/Protocol/target/generated-sources/java/com/hypixel/hytale/protocol/packets/connection",
    );
    if path.is_dir() {
        Some(path)
    } else {
        eprintln!(
            "skipping: generated connection sources are not present at {}",
            path.display()
        );
        None
    }
}

#[test]
fn real_connection_family_matches_connect_constants() {
    let Some(dir) = real_connection_dir() else {
        return;
    };
    let ir = connection::build(&dir).expect("connection IR builds");
    assert_eq!(ir.packets.len(), 8);
    assert_eq!(ir.enums.len(), 5);

    let struct_names: Vec<&str> = ir.structs.iter().map(|value| value.name.as_str()).collect();
    assert_eq!(
        struct_names,
        [
            "BoolParamValue",
            "DoubleParamValue",
            "FormattedMessage",
            "FormattedMessageImage",
            "HostAddress",
            "InstantData",
            "IntParamValue",
            "LongParamValue",
            "ParamValue",
            "PlayerSkin",
            "StringParamValue",
        ]
    );
    let formatted = ir
        .structs
        .iter()
        .find(|value| value.name == "FormattedMessage")
        .expect("FormattedMessage closure");
    let params = formatted
        .fields
        .iter()
        .find(|field| field.name == "params")
        .expect("params map");
    assert_eq!(params.kind, "map");
    assert_eq!(
        params.map.as_ref().expect("map info").value_type,
        "ParamValue"
    );

    let connect = ir
        .packets
        .iter()
        .find(|packet| packet.name == "Connect")
        .expect("Connect present");
    assert_eq!(connect.packet_id, 0);
    assert_eq!(connect.direction, "ToServer");
    assert_eq!(connect.channel, "Default");
    assert!(!connect.compressed);
    assert_eq!(connect.fixed_block_size, 30);
    assert_eq!(connect.variable_block_start, 46);
    assert_eq!(connect.variable_field_count, 4);
    assert_eq!(connect.max_size, 38056);

    let names: Vec<&str> = connect
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "protocolCrc",
            "protocolBuildNumber",
            "clientVersion",
            "clientType",
            "identityToken",
            "language",
            "referralData",
            "referralSource"
        ]
    );

    let version = &connect.fields[2];
    assert_eq!(version.fixed_offset, Some(9));
    assert_eq!(version.fixed_size, Some(20));
    let string = version.string.as_ref().expect("fixed string metadata");
    assert_eq!(string.length_prefix, "fixed");
    assert_eq!(string.fixed_length, Some(20));
    assert_eq!(string.encoding, "ASCII");

    let token = &connect.fields[4];
    assert!(token.nullable);
    assert_eq!(token.nullable_bit, Some(0));
    assert_eq!(token.variable_order, Some(0));
    assert_eq!(token.variable_offset_slot, Some(30));
    let string = token.string.as_ref().expect("variable string metadata");
    assert_eq!(string.max_length, Some(8192));

    let referral = &connect.fields[6];
    assert_eq!(referral.kind, "array");
    assert_eq!(
        referral.array.as_ref().expect("array metadata").max_length,
        Some(4096)
    );

    // Enums carry their explicit wire values.
    let client_type = ir
        .enums
        .iter()
        .find(|enumerated| enumerated.name == "ClientType")
        .expect("ClientType present");
    assert_eq!(client_type.values[0].name, "Game");
    assert_eq!(client_type.values[0].value, 0);
    assert_eq!(client_type.values[1].name, "Editor");
    assert_eq!(client_type.values[1].value, 1);
}

#[test]
fn unexpected_enum_shape_fails_with_the_file() {
    let error = connection::build(&fixture("unexpected")).expect_err("must fail");
    let message = error.to_string();
    assert!(message.contains("BadEnum.java"), "{message}");
}
