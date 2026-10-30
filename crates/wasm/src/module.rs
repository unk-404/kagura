//! Module decoding, validation, and metadata.
//!
//! Validation uses `wasmparser`; the feature gate is configurable and defaults
//! to the MVP core specification plus floating-point operators. Metadata is
//! then re-read from the same bytes to build the runtime-facing module.

use std::fmt;
use std::ops::Range;

use wasmparser::{Encoding, ExternalKind, Parser, Payload, TypeRef, Validator, WasmFeatures};

use crate::error::DecodeError;
use crate::memory::{AddressWidth, MemoryType};
use crate::value::{FuncType, ValType};

/// Returns the default v1 validation feature gate.
///
/// The gate is the WebAssembly 1.0 (MVP) core specification plus floating-point
/// operators. Post-MVP proposals (multi-value, bulk memory, reference types,
/// SIMD, threads, exceptions, GC, memory64, ...) are disabled and rejected with
/// a [`DecodeError`] whose [`DecodeError::missing_feature`] names the required
/// capability. Call [`Module::decode_with_features`] with
/// `default_features() | WasmFeatures::MEMORY64` (and an appropriate
/// [`crate::MemoryCap`]) to opt into memory64.
pub fn default_features() -> WasmFeatures {
    // MVP 本身包含 f32/f64；wasmparser 把浮点指令拆成了独立开关。
    WasmFeatures::FLOATS
}

/// A function defined in the module.
///
/// Imported functions are not represented here; [`Module::imports`] exposes
/// them separately. The MVP interpreter does not support imports yet, so an
/// instance can only be created from a module without imports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    type_index: u32,
    locals: Box<[ValType]>,
    body: Range<usize>,
    operators: Range<usize>,
}

impl Function {
    /// Returns the index into [`Module::types`] for this function.
    pub fn type_index(&self) -> u32 {
        self.type_index
    }

    /// Returns the local types declared after the parameters.
    pub fn locals(&self) -> &[ValType] {
        &self.locals
    }

    /// Returns the byte range of the function body (locals plus operators).
    pub fn body_range(&self) -> Range<usize> {
        self.body.clone()
    }

    /// Returns the byte range of the operator stream (after local declarations).
    pub fn operators_range(&self) -> Range<usize> {
        self.operators.clone()
    }
}

/// The kind of an exported item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    /// A function export.
    Func,
    /// A table export.
    Table,
    /// A linear memory export.
    Memory,
    /// A global export.
    Global,
}

impl ExportKind {
    /// Returns a stable lowercase name for diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Func => "func",
            Self::Table => "table",
            Self::Memory => "memory",
            Self::Global => "global",
        }
    }
}

/// An export declared by a module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    /// Export name.
    pub name: String,
    /// Export kind.
    pub kind: ExportKind,
    /// Index into the corresponding index space.
    pub index: u32,
}

/// The kind of an imported item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportKind {
    /// An imported function with its type index.
    Func {
        /// Index into the type section.
        type_index: u32,
    },
    /// An imported table.
    Table,
    /// An imported linear memory.
    Memory,
    /// An imported global.
    Global,
    /// An imported tag (exception handling).
    Tag,
}

impl ImportKind {
    /// Returns a stable lowercase name for diagnostics.
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

/// An import declared by a module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    /// Import module name.
    pub module: String,
    /// Import field name.
    pub name: String,
    /// Import kind.
    pub kind: ImportKind,
}

/// Aggregate metadata about a decoded module.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModuleStats {
    /// Size of the encoded module in bytes.
    pub byte_len: usize,
    /// Number of function types.
    pub type_count: u32,
    /// Number of imported items.
    pub import_count: u32,
    /// Number of functions defined by the code section.
    pub function_count: u32,
    /// Number of defined linear memories.
    pub memory_count: u32,
    /// Number of defined tables.
    pub table_count: u32,
    /// Number of defined globals.
    pub global_count: u32,
    /// Number of exports.
    pub export_count: u32,
    /// Number of data segments.
    pub data_segment_count: u32,
    /// Number of element segments.
    pub element_segment_count: u32,
    /// Total size in bytes of all function bodies.
    pub code_bytes: usize,
}

/// A validated WebAssembly module.
///
/// `Module` owns a copy of the original binary so that decoding is safe to do
/// from borrowed input. Validation happens once in [`Module::decode`]; the
/// interpreter re-decodes function bodies lazily from the stored bytes.
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
    /// Decodes and validates a WebAssembly binary module with the default v1
    /// feature gate.
    ///
    /// The default gate is the MVP core specification plus floating-point
    /// operators; see [`default_features`]. Invalid binaries produce
    /// [`DecodeError`] with a byte offset when the decoder reports one.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        Self::decode_with_features(bytes, default_features())
    }

    /// Decodes and validates with an explicit `wasmparser` feature set.
    ///
    /// This is the capability gate for targets that support more than the v1
    /// default. For example, enabling [`WasmFeatures::MEMORY64`] accepts 64-bit
    /// linear memories; the resulting [`MemoryType::address_width`] is then
    /// [`AddressWidth::W64`]. Runtime admission is still controlled by
    /// [`crate::MemoryCap`], so accepting a module never commits its declared
    /// memory.
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

    /// Returns the WebAssembly binary version (currently 1).
    pub fn version(&self) -> u16 {
        self.version
    }

    /// Returns the original encoded module bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the function types declared by the module.
    pub fn types(&self) -> &[FuncType] {
        &self.types
    }

    /// Returns the functions defined by the code section.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    /// Returns the linear memory types declared by the module.
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

    /// Returns the imports declared by the module.
    pub fn imports(&self) -> &[Import] {
        &self.imports
    }

    /// Returns the exports declared by the module.
    pub fn exports(&self) -> &[Export] {
        &self.exports
    }

    /// Returns the start function index, when present.
    pub fn start_function(&self) -> Option<u32> {
        self.start_function
    }

    /// Returns aggregate module statistics.
    pub fn stats(&self) -> ModuleStats {
        self.stats
    }

    /// Looks up a defined function by index.
    pub fn function(&self, index: u32) -> Option<&Function> {
        self.functions.get(index as usize)
    }

    /// Resolves the [`FuncType`] of a defined function.
    pub fn function_type(&self, function: &Function) -> Option<&FuncType> {
        self.types.get(function.type_index as usize)
    }

    /// Looks up an export by name.
    pub fn export(&self, name: &str) -> Option<&Export> {
        self.exports.iter().find(|export| export.name == name)
    }

    /// Returns the decoded operator bytes for a defined function.
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
