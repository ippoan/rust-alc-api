#[cfg(coverage)]
#[macro_export]
macro_rules! test_case {
    ($desc:expr, $body:expr) => {
        $body
    };
}

#[cfg(not(coverage))]
#[macro_export]
macro_rules! test_case {
    ($desc:expr, $body:expr) => {{
        print!("  ✅ {} ... ", $desc);
        std::io::Write::flush(&mut std::io::stdout()).ok();
        let val = $body;
        println!("OK");
        val
    }};
}

#[cfg(coverage)]
#[macro_export]
macro_rules! test_group {
    ($name:expr) => {};
}

#[cfg(not(coverage))]
#[macro_export]
macro_rules! test_group {
    ($name:expr) => {
        println!("\n📋 {}", $name);
    };
}
