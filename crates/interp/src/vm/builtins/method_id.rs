#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MethodOwner {
    Array,
    String,
    Number,
    Boolean,
    Object,
    Function,
    Date,
    Math,
}

macro_rules! define_builtin_methods {
    ($( $variant:ident => $name:literal ),* $(,)?) => {
        #[repr(u16)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum BuiltinMethodId {
            $( $variant, )*
        }

        impl BuiltinMethodId {
            pub(crate) fn from_name(name: &str) -> Option<Self> {
                Some(match name {
                    $( $name => Self::$variant, )*
                    _ => return None,
                })
            }

            pub(crate) const fn name(self) -> &'static str {
                match self {
                    $( Self::$variant => $name, )*
                }
            }
        }
    };
}

define_builtin_methods! {
    At => "at",
    Push => "push",
    Pop => "pop",
    Join => "join",
    Slice => "slice",
    IndexOf => "indexOf",
    LastIndexOf => "lastIndexOf",
    Includes => "includes",
    Concat => "concat",
    Reverse => "reverse",
    Sort => "sort",
    Fill => "fill",
    CopyWithin => "copyWithin",
    Flat => "flat",
    FlatMap => "flatMap",
    Map => "map",
    Filter => "filter",
    Find => "find",
    FindIndex => "findIndex",
    FindLast => "findLast",
    FindLastIndex => "findLastIndex",
    ForEach => "forEach",
    Every => "every",
    Some => "some",
    Reduce => "reduce",
    ReduceRight => "reduceRight",
    CharAt => "charAt",
    CharCodeAt => "charCodeAt",
    CodePointAt => "codePointAt",
    ToUpperCase => "toUpperCase",
    ToLowerCase => "toLowerCase",
    Trim => "trim",
    TrimStart => "trimStart",
    TrimEnd => "trimEnd",
    Replace => "replace",
    ReplaceAll => "replaceAll",
    StartsWith => "startsWith",
    EndsWith => "endsWith",
    Repeat => "repeat",
    Substring => "substring",
    Substr => "substr",
    LocaleCompare => "localeCompare",
    PadStart => "padStart",
    PadEnd => "padEnd",
    Split => "split",
    ToString => "toString",
    ToLocaleString => "toLocaleString",
    ValueOf => "valueOf",
    ToFixed => "toFixed",
    ToExponential => "toExponential",
    ToPrecision => "toPrecision",
    Call => "call",
    Apply => "apply",
    Bind => "bind",
    HasOwnProperty => "hasOwnProperty",
    IsPrototypeOf => "isPrototypeOf",
    PropertyIsEnumerable => "propertyIsEnumerable",
    Abs => "abs",
    Floor => "floor",
    Ceil => "ceil",
    Trunc => "trunc",
    Round => "round",
    Sqrt => "sqrt",
    Pow => "pow",
    Acos => "acos",
    Asin => "asin",
    Atan => "atan",
    Atan2 => "atan2",
    Cos => "cos",
    Sin => "sin",
    Tan => "tan",
    Exp => "exp",
    Log => "log",
    Log2 => "log2",
    Log10 => "log10",
    Cbrt => "cbrt",
    Fround => "fround",
    Clz32 => "clz32",
    Imul => "imul",
    Hypot => "hypot",
    Min => "min",
    Max => "max",
    Random => "random",
    GetTime => "getTime",
    ToISOString => "toISOString",
    GetYear => "getYear",
    GetUTCFullYear => "getUTCFullYear",
    GetFullYear => "getFullYear",
    GetUTCMonth => "getUTCMonth",
    GetMonth => "getMonth",
    GetUTCDate => "getUTCDate",
    GetDate => "getDate",
    GetUTCDay => "getUTCDay",
    GetDay => "getDay",
    GetUTCHours => "getUTCHours",
    GetHours => "getHours",
    GetUTCMinutes => "getUTCMinutes",
    GetMinutes => "getMinutes",
    GetUTCSeconds => "getUTCSeconds",
    GetSeconds => "getSeconds",
    GetUTCMilliseconds => "getUTCMilliseconds",
    GetMilliseconds => "getMilliseconds",
    GetTimezoneOffset => "getTimezoneOffset",
    SetTime => "setTime",
    IsNaN => "isNaN",
    IsFinite => "isFinite",
    IsInteger => "isInteger",
    IsSafeInteger => "isSafeInteger",
    ParseInt => "parseInt",
    ParseFloat => "parseFloat",
}
