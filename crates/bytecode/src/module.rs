use kagura_support::Span;

use crate::error::DecodeError;

#[derive(Clone, Debug, PartialEq)]
pub enum Const {
    Number(f64),
    String(String),
    Undefined,
    Null,
    True,
    False,
}

#[derive(Clone, Debug)]
pub struct FunctionProto {
    pub name: String,
    pub param_count: u16,
    pub local_count: u16,
    pub code: Vec<u8>,
    pub spans: Vec<Span>,
}

#[derive(Clone, Debug)]
pub struct Module {
    pub version: u16,
    pub constants: Vec<Const>,
    pub functions: Vec<FunctionProto>,
    pub entry: usize,
}

pub const MAGIC: [u8; 4] = *b"KGB1";
pub const VERSION: u16 = 4;
pub const MIN_VERSION: u16 = 1;

impl Module {
    pub fn find_function(&self, name: &str) -> Option<usize> {
        self.functions
            .iter()
            .position(|function| function.name == name)
    }

    pub fn span_at(&self, function: usize, pc: usize) -> Span {
        self.functions
            .get(function)
            .and_then(|function| function.spans.get(pc).copied())
            .unwrap_or_default()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&self.version.to_le_bytes());
        write_u32(&mut out, self.constants.len() as u32);
        for constant in &self.constants {
            match constant {
                Const::Number(value) => {
                    out.push(0);
                    out.extend_from_slice(&value.to_le_bytes());
                }
                Const::String(value) => {
                    out.push(1);
                    write_string(&mut out, value);
                }
                Const::Undefined => out.push(2),
                Const::Null => out.push(3),
                Const::True => out.push(4),
                Const::False => out.push(5),
            }
        }
        write_u32(&mut out, self.functions.len() as u32);
        write_u32(&mut out, self.entry as u32);
        for function in &self.functions {
            write_string(&mut out, &function.name);
            out.extend_from_slice(&function.param_count.to_le_bytes());
            out.extend_from_slice(&function.local_count.to_le_bytes());
            write_u32(&mut out, function.code.len() as u32);
            out.extend_from_slice(&function.code);
            write_u32(&mut out, function.spans.len() as u32);
            for span in &function.spans {
                out.extend_from_slice(&span.start.to_le_bytes());
                out.extend_from_slice(&span.end.to_le_bytes());
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::new(bytes);
        if reader.bytes(4)? != MAGIC {
            return Err(DecodeError::BadMagic);
        }
        let version = reader.u16()?;
        if !(MIN_VERSION..=VERSION).contains(&version) {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        let constant_count = reader.u32()? as usize;
        let mut constants = Vec::new();
        for _ in 0..constant_count {
            constants.push(match reader.u8()? {
                0 => Const::Number(reader.f64()?),
                1 => Const::String(reader.string()?),
                2 => Const::Undefined,
                3 => Const::Null,
                4 => Const::True,
                5 => Const::False,
                tag => return Err(DecodeError::InvalidConstant(tag)),
            });
        }
        let function_count = reader.u32()? as usize;
        let entry = reader.u32()? as usize;
        let mut functions = Vec::new();
        for _ in 0..function_count {
            let name = reader.string()?;
            let param_count = reader.u16()?;
            let local_count = reader.u16()?;
            let code_len = reader.u32()? as usize;
            let code = reader.bytes(code_len)?.to_vec();
            let span_count = reader.u32()? as usize;
            let mut spans = Vec::new();
            for _ in 0..span_count {
                spans.push(Span::new(reader.u32()?, reader.u32()?));
            }
            functions.push(FunctionProto {
                name,
                param_count,
                local_count,
                code,
                spans,
            });
        }
        if reader.remaining() != 0 {
            return Err(DecodeError::TrailingBytes);
        }
        if entry >= functions.len() {
            return Err(DecodeError::InvalidEntry);
        }
        Ok(Self {
            version,
            constants,
            functions,
            entry,
        })
    }
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    write_u32(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(len).ok_or(DecodeError::Truncated)?;
        let bytes = self
            .bytes
            .get(self.pos..end)
            .ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        let bytes: [u8; 2] = self
            .bytes(2)?
            .try_into()
            .map_err(|_| DecodeError::Truncated)?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        let bytes: [u8; 4] = self
            .bytes(4)?
            .try_into()
            .map_err(|_| DecodeError::Truncated)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn f64(&mut self) -> Result<f64, DecodeError> {
        let bytes: [u8; 8] = self
            .bytes(8)?
            .try_into()
            .map_err(|_| DecodeError::Truncated)?;
        Ok(f64::from_le_bytes(bytes))
    }

    fn string(&mut self) -> Result<String, DecodeError> {
        let len = self.u32()? as usize;
        let bytes = self.bytes(len)?;
        Ok(String::from_utf8(bytes.to_vec())?)
    }
}
