pub mod pb {
    #![allow(clippy::pedantic)]
    tonic::include_proto!("file_upload.v1");

    /// Encoded file descriptor set for gRPC reflection.
    pub const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("file_upload_descriptor");
}

pub mod auth;
pub mod config;
pub mod health;
pub mod interceptor;
pub mod rest;
pub mod service;
pub mod storage;
