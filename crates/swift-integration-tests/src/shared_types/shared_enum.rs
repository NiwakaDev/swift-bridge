#[swift_bridge::bridge]
mod ffi {
    enum EnumWithNoData {
        Variant1,
        Variant2,
    }

    extern "Rust" {
        fn reflect_enum_with_no_data(arg: EnumWithNoData) -> EnumWithNoData;
    }

    extern "Rust" {
        #[swift_bridge(Equatable)]
        type OpaqueRustForEnumTest;
        
        #[swift_bridge(init)]
        fn new() -> OpaqueRustForEnumTest;
    }

    enum EnumWithUnnamedData {
        TwoFields(String, OpaqueRustForEnumTest),
        OneField(i32),
        NoFields,
    }

    extern "Rust" {
        fn reflect_enum_with_unnamed_data(arg: EnumWithUnnamedData) -> EnumWithUnnamedData;
    }

    enum EnumWithNamedData {
        TwoFields { hello: String, data_u8: u8 },
        OneField { data_i32: i32 },
        NoFields,
    }

    extern "Rust" {
        fn reflect_enum_with_named_data(arg: EnumWithNamedData) -> EnumWithNamedData;
    }

    enum EnumWithOpaqueRust {
        Named { data: OpaqueRustForEnumTest },
        Unnamed(OpaqueRustForEnumTest)
    }

    extern "Rust" {
        fn reflect_enum_with_opaque_type(arg: EnumWithOpaqueRust) -> EnumWithOpaqueRust;
    }
}

fn reflect_enum_with_no_data(arg: ffi::EnumWithNoData) -> ffi::EnumWithNoData {
    arg
}

fn reflect_enum_with_unnamed_data(arg: ffi::EnumWithUnnamedData) -> ffi::EnumWithUnnamedData {
    arg
}

fn reflect_enum_with_named_data(arg: ffi::EnumWithNamedData) -> ffi::EnumWithNamedData {
    arg
}

fn reflect_enum_with_opaque_type(arg: ffi::EnumWithOpaqueRust) -> ffi::EnumWithOpaqueRust {
    arg
}

#[derive(PartialEq)]
pub struct OpaqueRustForEnumTest;

impl OpaqueRustForEnumTest {
    fn new() -> Self {
        OpaqueRustForEnumTest
    }
}
