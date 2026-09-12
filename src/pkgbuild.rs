use std::io::Read;

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    UnexpectedByte(u8),
    ExpectedUtf8(std::str::Utf8Error),
    MissingField(&'static str),
    UnknownField(String),
}

macros::make_pkgbuild_struct! {
    #[derive(Debug)]
    pub struct PkgBuild {
        required: [pkgname, pkgver, pkgrel, url],
        optional: [epoch, pkgdesc],
        multi: [license, source, makedepends],
        script: [package, build, prepare],
    }
}

#[derive(Debug)]
pub struct Script(String);

mod macros {
    macro_rules! make_pkgbuild_struct {
        ($(#[$attr:meta])* $vis:vis struct $name:ident {
            required: [$($req:ident),*],
            optional: [$($opt:ident),*],
            multi:    [$($mul:ident),*],
            script:   [$($scr:ident),*],
        }) => {
            $(#[$attr])*
            $vis struct $name {
                $($req: String,)*
                $($opt: Option<String>,)*
                $($mul: Vec<String>,)*
                $($scr: Script,)*
            }

            impl $name {
                $vis fn parse(r: impl Read) -> Result<Self, Error> {
                    let mut reader = Reader::new(r);
                    $(let mut $req = None;)*
                    $(let mut $opt = None;)*
                    $(let mut $mul = Vec::new();)*
                    $(let mut $scr = None;)*
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
                                    $(stringify!($req) => Self::parse_str_field(&mut reader, &mut $req)?,)*
                                    $(stringify!($opt) => Self::parse_str_field(&mut reader, &mut $opt)?,)*
                                    $(stringify!($mul) => Self::parse_list_field(&mut reader, &mut $mul)?,)*
                                    _ => {
                                        return Err(Error::UnknownField(key));
                                    }
                                }
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
                                        b'}' if count == 0 => { break; }
                                        b'{' => { buf.push(b); count += 1; }
                                        b'}' => { buf.push(b); count -= 1; }
                                        b => { buf.push(b); }
                                    }
                                }
                                let value = String::from_utf8(buf)?;
                                match key.as_str() {
                                    $(stringify!($scr) => $scr.replace(Script(value)).dummy(),)*
                                    _ => {
                                        return Err(Error::UnknownField(key));
                                    }
                                }
                            }
                            Some(b) => { return Err(Error::UnexpectedByte(b)); }
                            None => { return Err(Error::IO(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))); }
                        }
                    }
                    let mut this = Self {
                        $($req: $req.ok_or(Error::MissingField(stringify!($req)))?,)*
                        $($opt,)*
                        $($mul,)*
                        $($scr: $scr.ok_or(Error::MissingField(stringify!($scr)))?,)*
                    };
                    this.fixup();
                    Ok(this)
                }
                fn fixup(&mut self) {
                    let ac = aho_corasick::AhoCorasick::builder().build([
                        $(concat!("$", stringify!($req)),)*
                        $(concat!("$", stringify!($opt)),)*
                    ]).unwrap();
                    let members = [$(&self.$req,)* $(self.$opt.as_deref().unwrap_or_default(),)*];
                    $(let $req = ac.replace_all(&self.$req, &members);)*
                    $(let $opt = self.$opt.as_deref().map(|o| ac.replace_all(o, &members));)*
                    $(for f in self.$mul.iter_mut() { *f = ac.replace_all(f, &members); })*
                    $(self.$scr.0 = ac.replace_all(&self.$scr.0, &members);)*
                    $(self.$req = $req;)*
                    $(self.$opt = $opt;)*
                }
            }
        };
    }
    pub(crate) use make_pkgbuild_struct;
}

struct Reader<R> {
    inner: R,
    peeked: Option<u8>,
}

impl PkgBuild {
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
