// @generated — do not edit by hand.
#![allow(unexpected_cfgs)]
#![allow(clippy::empty_docs)]
#![allow(clippy::useless_borrows_in_formatting)]
#![allow(non_camel_case_types)]
#![allow(clippy::derivable_impls)]
pub use providers::v1::Provider;
pub use recipients::v1::Recipient;
pub use shares::v1::Share;
use std::collections::HashMap;
pub type PropertyMap = HashMap<String, serde_json::Value>;
pub mod providers {
    pub mod v1 {
        include!("./unitycatalog.providers.v1.rs");
    }
}
pub mod recipients {
    pub mod v1 {
        include!("./unitycatalog.recipients.v1.rs");
    }
}
pub mod shares {
    pub mod v1 {
        include!("./unitycatalog.shares.v1.rs");
    }
}
#[cfg(feature = "axum")]
pub mod extractors;
