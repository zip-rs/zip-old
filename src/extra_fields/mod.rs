pub trait ExtraFieldVersion {}
pub struct LocalHeaderVersion;
pub struct CentralHeaderVersion;

impl ExtraFieldVersion for LocalHeaderVersion {}
impl ExtraFieldVersion for CentralHeaderVersion {}

mod extended_timestamp;

pub use extended_timestamp::*;