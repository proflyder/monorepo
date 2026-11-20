use std::fs::File;
use std::io::{self, Write};

const RECORD_SIZE: usize = 10240;
const BLOCK_SIZE: usize = 512;

pub struct Archive {
    file: File,
    offset: usize,
}

impl Archive {
    pub fn create(path: &str) -> io::Result<Self> {
        let file = File::create(path)?;
        Ok(Self { file, offset: 0 })
    }

    pub fn add_file(&mut self, name: &str, data: &[u8]) -> io::Result<()> {
        let header = ustar_header(name, data.len())?;
        self.file.write_all(&header)?;
        self.file.write_all(data)?;
        let padding = (BLOCK_SIZE - data.len() % BLOCK_SIZE) % BLOCK_SIZE;
        if padding > 0 {
            self.file.write_all(&vec![0; padding])?;
        }
        self.offset += header.len() + data.len() + padding;
        Ok(())
    }

    pub fn close(mut self) -> io::Result<()> {
        let terminator = vec![0; 2 * BLOCK_SIZE];
        self.file.write_all(&terminator)?;
        self.offset += terminator.len();
        let padding = (RECORD_SIZE - self.offset % RECORD_SIZE) % RECORD_SIZE;
        if padding > 0 {
            self.file.write_all(&vec![0; padding])?;
        }
        self.file.flush()
    }
}

fn ustar_header(name: &str, size: usize) -> io::Result<Vec<u8>> {
    if name.len() > 100 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("tar entry name {name:?} is longer than 100 bytes"),
        ));
    }
    let mut header = vec![0u8; BLOCK_SIZE];
    header[0..name.len()].copy_from_slice(name.as_bytes());
    put_octal(&mut header[100..108], 0o644, 7);
    put_octal(&mut header[108..116], 0, 7);
    put_octal(&mut header[116..124], 0, 7);
    put_octal(&mut header[124..136], size as u64, 11);
    put_octal(&mut header[136..148], 0, 11);
    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    header[148..154].copy_from_slice(format!("{checksum:06o}").as_bytes());
    header[154] = 0;
    header[155] = b' ';
    Ok(header)
}

fn put_octal(field: &mut [u8], value: u64, digits: usize) {
    let text = format!("{value:0digits$o}", digits = digits);
    field[..digits].copy_from_slice(text.as_bytes());
    field[digits] = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_python_ustar() {
        let header = ustar_header("oci-layout", 31).expect("header");
        assert_eq!(&header[0..10], b"oci-layout");
        assert_eq!(&header[100..108], b"0000644\0");
        assert_eq!(&header[124..136], b"00000000037\0");
        assert_eq!(&header[257..265], b"ustar\x0000");
        assert_eq!(&header[329..337], &[0u8; 8]);
        let expected: u32 = header
            .iter()
            .enumerate()
            .map(|(index, byte)| {
                if (148..156).contains(&index) {
                    u32::from(b' ')
                } else {
                    u32::from(*byte)
                }
            })
            .sum();
        assert_eq!(&header[148..154], format!("{expected:06o}").as_bytes());
    }

    #[test]
    fn archives_pad_to_the_record_size() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("image.tar");
        let mut archive = Archive::create(&path.display().to_string()).expect("create");
        archive.add_file("index.json", b"{}").expect("add");
        archive.close().expect("close");
        let bytes = std::fs::read(path).expect("read");
        assert_eq!(bytes.len() % RECORD_SIZE, 0);
        assert_eq!(bytes.len(), RECORD_SIZE);
    }
}
