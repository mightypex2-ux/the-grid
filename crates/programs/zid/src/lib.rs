#![forbid(unsafe_code)]

mod zid;

pub use zid::{ZidDescriptor, ZidMessage};

#[cfg(test)]
mod tests;
