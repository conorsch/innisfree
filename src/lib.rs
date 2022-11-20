//! Innisfree, a project for exposing local network services
//! via a public cloud IP. The traffic is routed transparently
//! from a cloud VM to the local machine running `innisfree`
//! via an ad-hoc Wireguard tunnel. Multiple services can be
//! configured, via [crate::config::ServicePort].
//!
//! Right now, only TCP traffic is supported, but UDP support is planned.
//! As for cloud providers, only DigitalOcean is supported,
//! but adding others should be fairly straightforward.


#![warn(missing_docs)]
#[macro_use]
extern crate log;

pub mod config;
pub mod manager;
pub mod net;
pub mod proxy;
pub mod server;
pub mod ssh;
pub mod wg;
