pub mod pb {
    #![allow(clippy::pedantic)]
    tonic::include_proto!("file_upload.v1");
}

pub mod auth;
pub mod config;
pub mod health;
pub mod interceptor;
pub mod rest;
pub mod service;
pub mod storage;
