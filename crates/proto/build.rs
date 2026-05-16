fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = "proto";
    let proto_files = &[
        "proto/swarmos/v1/common.proto",
        "proto/swarmos/v1/session.proto",
        "proto/swarmos/v1/scheduler.proto",
        "proto/swarmos/v1/project_tools.proto",
        "proto/swarmos/v1/effects.proto",
        "proto/swarmos/v1/node_daemon.proto",
    ];

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(proto_files, &[proto_dir])?;

    Ok(())
}
