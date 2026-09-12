use std::io::Read;

use aho_corasick::AhoCorasick;

#[derive(Debug)]
pub struct Script(String);

#[derive(Debug)]
pub struct PkgBuild {
    pub pkgname: String,
    pub pkgver: String,
    pub pkgrel: String,
    pub epoch: Option<String>,
    pub pkgdesc: Option<String>,
    pub url: String,
    pub license: Vec<String>,
    pub source: Vec<String>,
    pub makedepends: Vec<String>,

    pub package: Script,
    pub build: Script,
    pub prepare: Script,
}

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    UnexpectedByte(u8),
    ExpectedUtf8(std::str::Utf8Error),
    MissingField(&'static str),
    UnknownField(String),
}

struct Reader<R> {
    inner: R,
    peeked: Option<u8>,
}

macro_rules! replace_members {
    ($self:expr) => {
        &[
            &$self.pkgname,
            &$self.pkgver,
            &$self.pkgrel,
            $self.epoch.as_deref().unwrap_or_default(),
            $self.pkgdesc.as_deref().unwrap_or_default(),
            &$self.url,
        ]
    };
}

macro_rules! replace {
    (String: $self:expr, $ac:expr, [$($field:ident),*]) => {
        $($self.$field = $ac.replace_all(&$self.$field, replace_members!($self)));*
    };
    (Option<String>: $self:expr, $ac:expr, [$($field:ident),*]) => {
        $(if let Some(f) = $self.$field.as_deref() {
            $self.$field = Some($ac.replace_all(
                f,
                replace_members!($self),
            ))
        });*
    };
    (Vec<String>: $self:expr, $ac:expr, [$($field:ident),*]) => {
        $(for f in $self.$field.iter_mut() {
            *f = $ac.replace_all(f, replace_members!($self))
        });*
    };
    (Script: $self:expr, $ac:expr, [$($field:ident),*]) => {
        $($self.$field.0 = $ac.replace_all(&$self.$field.0, replace_members!($self) ));*
    }
}

impl PkgBuild {
    pub fn parse(r: impl Read) -> Result<Self, Error> {
        let mut reader = Reader::new(r);

        let mut pkgname = None;
        let mut pkgver = None;
        let mut pkgrel = None;
        let mut epoch = None;
        let mut pkgdesc = None;
        let mut url = None;
        let mut license = Vec::new();
        let mut source = Vec::new();
        let mut makedepends = Vec::new();

        let mut package = None;
        let mut build = None;
        let mut prepare = None;

        loop {
            let mut buf = Vec::new();
            reader.skip_while(|b| b.is_ascii_whitespace())?;
            reader.read_while(&mut buf, |b| b.is_ascii_alphabetic())?;
            if buf.is_empty() {
                if reader.peek_byte()?.is_some_and(|b| b == b'#') {
                    reader.skip_while(|b| b != b'\n')?;
                    let _ = reader.read_byte();
                    continue;
                }
                break;
            }
            let key = unsafe { String::from_utf8_unchecked(buf) };

            reader.skip_while(|b| b.is_ascii_whitespace())?;
            let symbol = reader.read_byte()?;
            match symbol {
                Some(b'=') => {
                    match key.as_str() {
                        "pkgname" => Self::parse_str_field(&mut reader, &mut pkgname)?,
                        "pkgver" => Self::parse_str_field(&mut reader, &mut pkgver)?,
                        "pkgrel" => Self::parse_str_field(&mut reader, &mut pkgrel)?,
                        "epoch" => Self::parse_str_field(&mut reader, &mut epoch)?,
                        "pkgdesc" => Self::parse_str_field(&mut reader, &mut pkgdesc)?,
                        "url" => Self::parse_str_field(&mut reader, &mut url)?,
                        "license" => Self::parse_list_field(&mut reader, &mut license)?,
                        "source" => Self::parse_list_field(&mut reader, &mut source)?,
                        "makedepends" => Self::parse_list_field(&mut reader, &mut makedepends)?,
                        _ => {
                            return Err(Error::UnknownField(key));
                        }
                    };
                }
                Some(b'(') => {
                    reader.expect_byte(b')')?;
                    reader.skip_while(|b| b.is_ascii_whitespace())?;
                    reader.expect_byte(b'{')?;
                    reader.skip_while(|b| b.is_ascii_whitespace())?;
                    let mut count = 0;
                    let mut buf = Vec::new();
                    while let Some(b) = reader.read_byte()? {
                        match b {
                            b'}' if count == 0 => {
                                break;
                            }
                            b'{' => {
                                buf.push(b);
                                count += 1;
                            }
                            b'}' => {
                                buf.push(b);
                                count -= 1;
                            }
                            b => {
                                buf.push(b);
                            }
                        }
                    }
                    let value = String::from_utf8(buf)?;
                    match key.as_str() {
                        "package" => package.replace(Script(value)).dummy(),
                        "build" => build.replace(Script(value)).dummy(),
                        "prepare" => prepare.replace(Script(value)).dummy(),
                        _ => {
                            return Err(Error::UnknownField(key));
                        }
                    };
                }
                Some(b) => {
                    return Err(Error::UnexpectedByte(b));
                }
                None => {
                    return Err(Error::IO(std::io::Error::from(
                        std::io::ErrorKind::UnexpectedEof,
                    )));
                }
            }
        }

        let mut this = Self {
            pkgname: pkgname.ok_or(Error::MissingField("pkgname"))?,
            pkgver: pkgver.ok_or(Error::MissingField("pkgver"))?,
            pkgrel: pkgrel.ok_or(Error::MissingField("pkgrel"))?,
            epoch,
            pkgdesc,
            url: url.ok_or(Error::MissingField("url"))?,
            license,
            source,
            makedepends,
            package: package.ok_or(Error::MissingField("package"))?,
            build: build.ok_or(Error::MissingField("build"))?,
            prepare: prepare.ok_or(Error::MissingField("prepare"))?,
        };
        this.fixup();
        Ok(this)
    }

    fn parse_str_field<R: Read>(
        reader: &mut Reader<R>,
        value: &mut Option<String>,
    ) -> Result<(), Error> {
        let mut buf = Vec::new();
        reader.skip_while(|b| b.is_ascii_whitespace() && b != b'\n')?;
        let start = reader.peek_byte()?;
        match start {
            Some(b'"') => {
                let _ = reader.read_byte();
                reader.read_while(&mut buf, |b| b != b'"')?;
                let _ = reader.read_byte();
            }
            Some(b'\'') => {
                let _ = reader.read_byte();
                reader.read_while(&mut buf, |b| b != b'\'')?;
                let _ = reader.read_byte();
            }
            Some(b'\n') => {
                let _ = reader.read_byte();
                value.take();
                return Ok(());
            }
            _ => {
                reader.read_while(&mut buf, |b| b != b'\n')?;
            }
        }
        buf.pop_if(|b| *b == b'\r');
        let v = String::from_utf8(buf)?;
        value.replace(v);
        Ok(())
    }

    fn parse_list_field<R: Read>(
        reader: &mut Reader<R>,
        value: &mut Vec<String>,
    ) -> Result<(), Error> {
        let mut buf = Vec::new();
        reader.skip_while(|b| b.is_ascii_whitespace())?;
        reader.expect_byte(b'(')?;
        reader.skip_while(|b| b.is_ascii_whitespace())?;
        while let Some(b) = reader.peek_byte()? {
            match b {
                b')' => {
                    let _ = reader.read_byte();
                    break;
                }
                b'"' => {
                    let _ = reader.read_byte();
                    reader.read_while(&mut buf, |b| b != b'"')?;
                    let _ = reader.read_byte();
                }
                b'\'' => {
                    let _ = reader.read_byte();
                    reader.read_while(&mut buf, |b| b != b'\'')?;
                    let _ = reader.read_byte();
                }
                _ => {
                    reader.read_while(&mut buf, |b| !b.is_ascii_whitespace() && b != b')')?;
                }
            }
            buf.pop_if(|b| *b == b'\r');
            let v = String::from_utf8(buf)?;
            value.push(v);
            buf = Vec::new();
            reader.skip_while(|b| b.is_ascii_whitespace())?;
        }
        Ok(())
    }

    fn fixup(&mut self) {
        let ac = AhoCorasick::builder()
            .build([
                "$pkgname", "$pkgver", "$pkgrel", "$epoch", "$pkgdesc", "$url",
            ])
            .unwrap();
        replace!(String: self, ac, [pkgname, pkgver, pkgrel, url]);
        replace!(Option<String>: self, ac, [epoch, pkgdesc]);
        replace!(Vec<String>: self, ac, [license, source, makedepends]);
        replace!(Script: self, ac, [package, build, prepare]);
        self.pkgname = self.pkgname.replace("pkgver", "");
    }
}

impl<R: Read> Reader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            peeked: None,
        }
    }

    fn read_byte(&mut self) -> std::io::Result<Option<u8>> {
        if let Some(b) = self.peeked.take() {
            return Ok(Some(b));
        }
        let mut buf = [0];
        match self.inner.read(&mut buf)? {
            0 => Ok(None),
            _ => Ok(Some(buf[0])),
        }
    }

    fn peek_byte(&mut self) -> std::io::Result<Option<u8>> {
        if self.peeked.is_none() {
            self.peeked = self.read_byte()?;
        }
        Ok(self.peeked)
    }

    fn read_while(&mut self, buf: &mut Vec<u8>, pred: impl Fn(u8) -> bool) -> std::io::Result<()> {
        while let Some(byte) = self.peek_byte()? {
            if !(pred)(byte) {
                break;
            }
            buf.push(byte);
            let _ = self.read_byte();
        }
        Ok(())
    }

    fn skip_while(&mut self, pred: impl Fn(u8) -> bool) -> std::io::Result<()> {
        while let Some(byte) = self.peek_byte()? {
            if !(pred)(byte) {
                break;
            }
            let _ = self.read_byte();
        }
        Ok(())
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), Error> {
        let actual = self.read_byte()?;
        let Some(actual) = actual else {
            return Err(Error::IO(std::io::Error::from(
                std::io::ErrorKind::UnexpectedEof,
            )));
        };
        if actual != expected {
            return Err(Error::UnexpectedByte(actual));
        }
        Ok(())
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::IO(value)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(value: std::string::FromUtf8Error) -> Self {
        Self::ExpectedUtf8(value.utf8_error())
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(value: std::str::Utf8Error) -> Self {
        Self::ExpectedUtf8(value)
    }
}

trait Dummy {
    fn dummy(&self) -> ();
}

impl<T> Dummy for T {
    fn dummy(&self) {}
}
