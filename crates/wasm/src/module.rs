//! Module decoding/validation via `wasmparser`, plus runtime-facing metadata.

use std::fmt;
use std::ops::Range;

use wasmparser::{Encoding, ExternalKind, Parser, Payload, TypeRef, Validator, WasmFeatures};

use crate::error::DecodeError;
use crate::memory::{AddressWidth, MemoryType};
use crate::value::{FuncType, ValType};

/// Default v1 gate: MVP plus floats. memory64 and other proposals stay off.
pub fn default_features() -> WasmFeatures {
    // MVP 本身包含 f32/f64；wasmparser 把浮点指令拆成了独立开关。
    WasmFeatures::FLOATS
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    type_index: u32,
    locals: Box<[ValType]>,
    body: Range<usize>,
    operators: Range<usize>,
}

impl Function {
    pub fn type_index(&self) -> u32 {
        self.type_index
    }

    pub fn locals(&self) -> &[ValType] {
        &self.locals
    }

    pub fn body_range(&self) -> Range<usize> {
        self.body.clone()
    }

    pub fn operators_range(&self) -> Range<usize> {
        self.operators.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Func,
    Table,
    Memory,
    Global,
}

impl ExportKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Func => "func",
            Self::Table => "table",
            Self::Memory => "memory",
            Self::Global => "global",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportKind {
    Func { type_index: u32 },
    Table,
    Memory,
    Global,
    Tag,
}

impl ImportKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Func { .. } => "func",
            Self::Table => "table",
            Self::Memory => "memory",
            Self::Global => "global",
            Self::Tag => "tag",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub kind: ImportKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModuleStats {
    pub byte_len: usize,
    pub type_count: u32,
    pub import_count: u32,
    pub function_count: u32,
    pub memory_count: u32,
    pub table_count: u32,
    pub global_count: u32,
    pub export_count: u32,
    pub data_segment_count: u32,
    pub element_segment_count: u32,
    pub code_bytes: usize,
}

#[derive(Clone)]
pub struct Module {
    bytes: Vec<u8>,
    version: u16,
    types: Vec<FuncType>,
    functions: Vec<Function>,
    memories: Vec<MemoryType>,
    imports: Vec<Import>,
    exports: Vec<Export>,
    start_function: Option<u32>,
    stats: ModuleStats,
}

impl Module {
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        Self::decode_with_features(bytes, default_features())
    }

    /// Decode with an explicit capability gate. Enabling memory64 only admits
    /// the module; [`crate::MemoryCap`] still governs actual memory admission.
    pub fn decode_with_features(bytes: &[u8], features: WasmFeatures) -> Result<Self, DecodeError> {
        // 第一遍：完整校验。非法模块与未启用的 capability 都在这里失败；
        // capability 错误会通过 DecodeError::missing_feature 暴露。
        let mut validator = Validator::new_with_features(features);
        validator.validate_all(bytes)?;

        let mut module = Self {
            bytes: bytes.to_vec(),
            version: 0,
            types: Vec::new(),
            functions: Vec::new(),
            memories: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            start_function: None,
            stats: ModuleStats::default(),
        };
        let mut function_types: Vec<u32> = Vec::new();
        let mut table_count = 0u32;
        let mut global_count = 0u32;
        let mut data_segment_count = 0u32;
        let mut element_segment_count = 0u32;

        // 第二遍：读取元数据。校验已通过，这里仍把解码错误向上传播以防御损坏输入。
        for payload in Parser::new(0).parse_all(bytes) {
            match payload? {
                Payload::Version { num, encoding, .. } => {
                    if encoding != Encoding::Module {
                        return Err(DecodeError::new(
                            "component-model binaries are not supported",
                        ));
                    }
                    module.version = num;
                }
                Payload::TypeSection(reader) => {
                    for func_type in reader.into_iter_err_on_gc_types() {
                        let func_type = func_type?;
                        let params = func_type
                            .params()
                            .iter()
                            .copied()
                            .map(convert_val_type)
                            .collect::<Result<Vec<_>, _>>()?;
                        let results = func_type
                            .results()
                            .iter()
                            .copied()
                            .map(convert_val_type)
                            .collect::<Result<Vec<_>, _>>()?;
                        module.types.push(FuncType::new(params, results));
                    }
                }
                Payload::ImportSection(reader) => {
                    for import in reader.into_imports() {
                        let import = import?;
                        module.imports.push(Import {
                            module: import.module.to_string(),
                            name: import.name.to_string(),
                            kind: convert_import_kind(import.ty),
                        });
                    }
                }
                Payload::FunctionSection(reader) => {
                    for type_index in reader {
                        function_types.push(type_index?);
                    }
                }
                Payload::TableSection(reader) => {
                    table_count = reader.count();
                }
                Payload::MemorySection(reader) => {
                    for memory in reader {
                        module.memories.push(convert_memory_type(memory?)?);
                    }
                }
                Payload::GlobalSection(reader) => {
                    global_count = reader.count();
                }
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export?;
                        module.exports.push(Export {
                            name: export.name.to_string(),
                            kind: convert_export_kind(export.kind)?,
                            index: export.index,
                        });
                    }
                }
                Payload::StartSection { func, .. } => {
                    module.start_function = Some(func);
                }
                Payload::ElementSection(reader) => {
                    element_segment_count = reader.count();
                }
                Payload::DataSection(reader) => {
                    data_segment_count = reader.count();
                }
                Payload::CodeSectionEntry(body) => {
                    let code_index = module.functions.len();
                    let type_index = *function_types.get(code_index).ok_or_else(|| {
                        DecodeError::new(format!(
                            "code section has more bodies than the function section declares \
                             (unexpected body {code_index})"
                        ))
                    })?;
                    module
                        .functions
                        .push(decode_function(&module.bytes, body, type_index)?);
                }
                // Custom sections, data-count markers, and section boundaries
                // carry no runtime metadata for this foundation.
                _ => {}
            }
        }

        if module.functions.len() != function_types.len() {
            return Err(DecodeError::new(format!(
                "code section has {} bodies but function section declares {} functions",
                module.functions.len(),
                function_types.len()
            )));
        }

        module.stats = ModuleStats {
            byte_len: module.bytes.len(),
            type_count: u32::try_from(module.types.len()).unwrap_or(u32::MAX),
            import_count: u32::try_from(module.imports.len()).unwrap_or(u32::MAX),
            function_count: u32::try_from(module.functions.len()).unwrap_or(u32::MAX),
            memory_count: u32::try_from(module.memories.len()).unwrap_or(u32::MAX),
            table_count,
            global_count,
            export_count: u32::try_from(module.exports.len()).unwrap_or(u32::MAX),
            data_segment_count,
            element_segment_count,
            code_bytes: module.functions.iter().map(|f| f.body.len()).sum(),
        };
        Ok(module)
    }

    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn types(&self) -> &[FuncType] {
        &self.types
    }

    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    pub fn memories(&self) -> &[MemoryType] {
        &self.memories
    }

    /// Returns the widest address width among the module's memories.
    ///
    /// Defaults to [`AddressWidth::W32`] for modules without memories. This is
    /// the module-side default for [`crate::Instance::new`] so a memory64
    /// module is not accidentally constrained by a 32-bit cap.
    pub fn memory_address_width(&self) -> AddressWidth {
        if self
            .memories
            .iter()
            .any(|memory| memory.address_width == AddressWidth::W64)
        {
            AddressWidth::W64
        } else {
            AddressWidth::W32
        }
    }

    pub fn imports(&self) -> &[Import] {
        &self.imports
    }

    pub fn exports(&self) -> &[Export] {
        &self.exports
    }

    pub fn start_function(&self) -> Option<u32> {
        self.start_function
    }

    pub fn stats(&self) -> ModuleStats {
        self.stats
    }

    pub fn function(&self, index: u32) -> Option<&Function> {
        self.functions.get(index as usize)
    }

    pub fn function_type(&self, function: &Function) -> Option<&FuncType> {
        self.types.get(function.type_index as usize)
    }

    pub fn export(&self, name: &str) -> Option<&Export> {
        self.exports.iter().find(|export| export.name == name)
    }

    pub fn operators_bytes(&self, function: &Function) -> &[u8] {
        &self.bytes[function.operators.clone()]
    }
}

impl fmt::Debug for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Module")
            .field("version", &self.version)
            .field("byte_len", &self.bytes.len())
            .field("types", &self.types.len())
            .field("functions", &self.functions.len())
            .field("memories", &self.memories.len())
            .field("imports", &self.imports.len())
            .field("exports", &self.exports.len())
            .finish()
    }
}

// 把单个函数体解码成运行期 Function；局部变量只存类型，指令留给执行器惰性读取。
fn decode_function(
    bytes: &[u8],
    body: wasmparser::FunctionBody<'_>,
    type_index: u32,
) -> Result<Function, DecodeError> {
    let mut locals = Vec::new();
    // 局部变量声明按 (count, type) 成组编码；校验器已限制总数。
    for local in body.get_locals_reader()? {
        let (count, ty) = local?;
        let ty = convert_val_type(ty)?;
        let count = usize::try_from(count)
            .map_err(|_| DecodeError::new("local count overflows the host usize"))?;
        locals.extend(std::iter::repeat_n(ty, count));
    }
    let body_range = to_usize_range(body.range(), bytes.len(), "function body")?;
    let operators_start = body.get_binary_reader_for_operators()?.original_position();
    let operators_start = usize::try_from(operators_start)
        .map_err(|_| DecodeError::new("operator offset overflows the host usize"))?;
    if operators_start < body_range.start || operators_start > body_range.end {
        return Err(DecodeError::new(
            "decoded operator stream starts outside the function body",
        ));
    }
    Ok(Function {
        type_index,
        locals: locals.into_boxed_slice(),
        body: body_range.clone(),
        operators: operators_start..body_range.end,
    })
}

// 模块 byte range 是 u64，本 crate 的 bytecode 切片用 host range；转换必须 checked。
fn to_usize_range(range: Range<u64>, len: usize, what: &str) -> Result<Range<usize>, DecodeError> {
    let start = usize::try_from(range.start)
        .map_err(|_| DecodeError::new(format!("{what} start overflows the host usize")))?;
    let end = usize::try_from(range.end)
        .map_err(|_| DecodeError::new(format!("{what} end overflows the host usize")))?;
    if start > end || end > len {
        return Err(DecodeError::new(format!(
            "{what} range {start}..{end} is out of bounds for {len} bytes"
        )));
    }
    Ok(start..end)
}

// 校验门禁允许的 ValType 映射；V128/GC 引用等尚未进入 v1 值模型时明确报错。
fn convert_val_type(ty: wasmparser::ValType) -> Result<ValType, DecodeError> {
    match ty {
        wasmparser::ValType::I32 => Ok(ValType::I32),
        wasmparser::ValType::I64 => Ok(ValType::I64),
        wasmparser::ValType::F32 => Ok(ValType::F32),
        wasmparser::ValType::F64 => Ok(ValType::F64),
        wasmparser::ValType::Ref(reference) if reference == wasmparser::RefType::FUNCREF => {
            Ok(ValType::FuncRef)
        }
        wasmparser::ValType::Ref(reference) if reference == wasmparser::RefType::EXTERNREF => {
            Ok(ValType::ExternRef)
        }
        other => Err(DecodeError::new(format!(
            "unsupported WebAssembly value type: {other:?}"
        ))),
    }
}

// 把解码器 MemoryType 转成运行期类型。memory64 映射到 W64，保留后续执行路径的
// 架构钩子；默认门禁仍会在更早阶段拒绝它。shared memory/custom page size 未支持。
fn convert_memory_type(memory: wasmparser::MemoryType) -> Result<MemoryType, DecodeError> {
    if memory.shared {
        return Err(DecodeError::new(
            "shared memories (threads) are not supported by the v1 foundation",
        ));
    }
    if memory.page_size_log2.is_some() {
        return Err(DecodeError::new(
            "custom page sizes are not supported by the v1 foundation",
        ));
    }
    let address_width = if memory.memory64 {
        AddressWidth::W64
    } else {
        AddressWidth::W32
    };
    Ok(MemoryType::with_address_width(
        memory.initial,
        memory.maximum,
        address_width,
    ))
}

fn convert_import_kind(ty: TypeRef) -> ImportKind {
    match ty {
        TypeRef::Func(type_index) | TypeRef::FuncExact(type_index) => {
            ImportKind::Func { type_index }
        }
        TypeRef::Table(_) => ImportKind::Table,
        TypeRef::Memory(_) => ImportKind::Memory,
        TypeRef::Global(_) => ImportKind::Global,
        TypeRef::Tag(_) => ImportKind::Tag,
    }
}

fn convert_export_kind(kind: ExternalKind) -> Result<ExportKind, DecodeError> {
    match kind {
        ExternalKind::Func => Ok(ExportKind::Func),
        ExternalKind::Table => Ok(ExportKind::Table),
        ExternalKind::Memory => Ok(ExportKind::Memory),
        ExternalKind::Global => Ok(ExportKind::Global),
        ExternalKind::Tag => Err(DecodeError::new(
            "tag exports (exception handling) are not supported",
        )),
        ExternalKind::FuncExact => {
            Err(DecodeError::new("exact function exports are not supported"))
        }
    }
}
