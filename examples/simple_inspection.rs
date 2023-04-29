use std::io;

const NAME: &str = "examples/example.zip";

fn main() -> io::Result<()> {
    for file in zip::files(std::fs::File::open(NAME)?)? {
        if let Ok(s) = std::str::from_utf8(file?.name()) {
            println!("{s}");
        }
    }
    Ok(())
}
