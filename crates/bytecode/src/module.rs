use kagura_support::Span;

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
pub const VERSION: u16 = 1;

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
}
