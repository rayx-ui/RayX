//! Whether rayx_app has the dev tools, as the test expects (`EXPECT_DEVTOOLS=1`): it does not
//! compile otherwise.

const EXPECTED: bool = match option_env!("EXPECT_DEVTOOLS") {
    Some(value) => !value.is_empty(),
    None => false,
};

const _: () = assert!(rayx_app::devtools_enabled() == EXPECTED);
