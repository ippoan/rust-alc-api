macro_rules! test_case {
    ($desc:expr, $body:expr) => {{
        print!("  ✅ {} ... ", $desc);
        std::io::Write::flush(&mut std::io::stdout()).ok();
        let val = $body;
        println!("OK");
        val
    }};
}

macro_rules! test_group {
    ($name:expr) => {
        println!("\n📋 {}", $name);
    };
}

macro_rules! test_section {
    ($name:expr) => {
        println!("\n  ── {} ──", $name);
    };
}

macro_rules! test_info {
    ($($arg:tt)*) => {
        println!("    💡 {}", format!($($arg)*));
    };
}
