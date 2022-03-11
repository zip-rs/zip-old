macro_rules! errors {
    { $($name:ident $kind:ident $desc:literal)* } => {
        $(

            #[derive(Debug)]
            pub struct $name(pub(crate) ());
            impl core::fmt::Display for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str($desc)
                }
            }
            impl std::error::Error for $name {}
            impl From<$name> for std::io::Error {
                fn from(_: $name) -> Self {
                    Self::new(std::io::ErrorKind::$kind, $name(()))
                }
            }
        )*
    };
}
errors! {
    NotAnArchive InvalidData "the source data is not a zip archive"
    DiskMismatch NotFound "the requested resource was located on another disk"
    MethodNotSupported Unsupported "the requested resource used an unsupported compression method"
}
