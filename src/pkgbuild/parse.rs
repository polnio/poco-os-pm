use super::PkgBuild;
use anyhow::{Result, bail};
use std::io::Read;

pub(crate) mod macros {
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
                $vis fn parse(r: impl std::io::Read) -> anyhow::Result<Self> {
                    let mut reader = parse::Reader::new(r);
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
                                        anyhow::bail!("Unknown field `{key}`");
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
                                    $(stringify!($scr) => { $scr.replace(Script(value)); })*
                                    _ => {
                                        anyhow::bail!("Unknown script `{key}`");
                                    }
                                }
                            }
                            Some(b) => { anyhow::bail!("Unexpected byte `{b:x}`"); }
                            None => { anyhow::bail!("Unexpected EOF"); }
                        }
                    }
                    let mut this = Self {
                        $($req: $req.ok_or_else(|| anyhow::anyhow!(concat!("Missing field `",stringify!($req),"`")))?,)*
                        $($opt,)*
                        $($mul,)*
                        $($scr: $scr.ok_or_else(|| anyhow::anyhow!(concat!("Missing script `",stringify!($scr),"`")))?,)*
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

pub(crate) struct Reader<R> {
    inner: R,
    peeked: Option<u8>,
}

impl PkgBuild {
    pub(crate) fn parse_str_field<R: Read>(
        reader: &mut Reader<R>,
        value: &mut Option<String>,
    ) -> Result<()> {
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

    pub(crate) fn parse_list_field<R: Read>(
        reader: &mut Reader<R>,
        value: &mut Vec<String>,
    ) -> Result<()> {
        let mut buf = Vec::new();
        reader.skip_while(|b| b.is_ascii_whitespace())?;
        reader.expect_byte(b'(')?;
        reader.skip_while(|b| b.is_ascii_whitespace())?;
        while let Some(b) = reader.peek_byte()? {
            match b {
                b'#' => {
                    reader.skip_while(|b| b != b'\n')?;
                    reader.skip_while(|b| b.is_ascii_whitespace())?;
                    continue;
                }
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
    pub(crate) fn new(inner: R) -> Self {
        Self {
            inner,
            peeked: None,
        }
    }

    pub(crate) fn read_byte(&mut self) -> std::io::Result<Option<u8>> {
        if let Some(b) = self.peeked.take() {
            return Ok(Some(b));
        }
        let mut buf = [0];
        match self.inner.read(&mut buf)? {
            0 => Ok(None),
            _ => Ok(Some(buf[0])),
        }
    }

    pub(crate) fn peek_byte(&mut self) -> std::io::Result<Option<u8>> {
        if self.peeked.is_none() {
            self.peeked = self.read_byte()?;
        }
        Ok(self.peeked)
    }

    pub(crate) fn read_while(
        &mut self,
        buf: &mut Vec<u8>,
        pred: impl Fn(u8) -> bool,
    ) -> std::io::Result<()> {
        while let Some(byte) = self.peek_byte()? {
            if !(pred)(byte) {
                break;
            }
            buf.push(byte);
            let _ = self.read_byte();
        }
        Ok(())
    }

    pub(crate) fn skip_while(&mut self, pred: impl Fn(u8) -> bool) -> std::io::Result<()> {
        while let Some(byte) = self.peek_byte()? {
            if !(pred)(byte) {
                break;
            }
            let _ = self.read_byte();
        }
        Ok(())
    }

    pub(crate) fn expect_byte(&mut self, expected: u8) -> Result<()> {
        let actual = self.read_byte()?;
        let Some(actual) = actual else {
            bail!("Unexpected EOF");
        };
        if actual != expected {
            bail!("Unexpected byte `{actual:x}`");
        }
        Ok(())
    }
}
