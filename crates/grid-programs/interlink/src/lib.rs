#![forbid(unsafe_code)]

pub mod interlink;

pub use interlink::{ChannelId, InterlinkDescriptor, ZMessage};

#[cfg(test)]
mod tests;
