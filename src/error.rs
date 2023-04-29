macro_rules! errors {
    { $($name:ident $kind:ident $desc:literal)* } => {
        $(

            #[derive(Debug, Clone)]
            #[doc = $desc]
            pub struct $name(pub(crate) ());
            impl core::fmt::Display for $name {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    f.write_str($desc)
                }
            }
            #[cfg(feature = "std")]
            impl std::error::Error for $name {}
            #[cfg(feature = "std")]
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
    FileLocked PermissionDenied "the zip file must be decrypted with a password before being read"
}
