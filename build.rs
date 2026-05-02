use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    tonic_prost_build::configure()
        .file_descriptor_set_path(out_dir.join("file_upload_descriptor.bin"))
        .compile_protos(&["proto/file_upload.proto"], &["proto/"])?;
    Ok(())
}
