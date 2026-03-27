pub mod kudgivt;
pub mod kudguri;
pub mod work_segments;

use std::io::Read;

/// ZIP バイト列を展開し、(ファイル名, バイト列) のリストを返す
pub fn extract_zip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, anyhow::Error> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;
    let mut files = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        files.push((name, contents));
    }
    Ok(files)
}

/// Shift-JIS バイト列を UTF-8 文字列に変換
pub fn decode_shift_jis(bytes: &[u8]) -> String {
    let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
    decoded.into_owned()
}

/// 運行NOでCSVデータをグループ化
/// 各CSVファイルから運行NOを抽出し、運行NO→行データのマップを返す
pub fn group_csv_by_unko_no(csv_text: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut lines = csv_text.lines();
    let _header = lines.next(); // skip header
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        // 運行NO is always the first column
        if let Some(unko_no) = line.split(',').next() {
            map.entry(unko_no.to_string())
                .or_default()
                .push(line.to_string());
        }
    }
    map
}

/// CSVテキストのヘッダー行を返す
pub fn csv_header(csv_text: &str) -> Option<&str> {
    csv_text.lines().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_zip() {
        test_group!("CSVパーサー");
        test_case!("ZIP展開", {
            use std::io::Write;
            let mut buf = std::io::Cursor::new(Vec::new());
            {
                let mut zip = zip::ZipWriter::new(&mut buf);
                let opts = zip::write::SimpleFileOptions::default();
                zip.start_file("test.txt", opts).unwrap();
                zip.write_all(b"hello world").unwrap();
                zip.start_file("sub/data.csv", opts).unwrap();
                zip.write_all(b"col1,col2\na,b").unwrap();
                zip.finish().unwrap();
            }
            let files = extract_zip(&buf.into_inner()).unwrap();
            assert_eq!(files.len(), 2);
            assert_eq!(files[0].0, "test.txt");
            assert_eq!(files[0].1, b"hello world");
            assert_eq!(files[1].0, "sub/data.csv");
        });
    }

    #[test]
    fn test_extract_zip_invalid() {
        test_group!("CSVパーサー");
        test_case!("不正なZIPでエラー", {
            assert!(extract_zip(b"not a zip").is_err());
        });
    }

    #[test]
    fn test_decode_shift_jis() {
        test_group!("CSVパーサー");
        test_case!("Shift-JISデコード", {
            let sjis_bytes = encoding_rs::SHIFT_JIS.encode("テスト").0.to_vec();
            assert_eq!(decode_shift_jis(&sjis_bytes), "テスト");
        });
    }

    #[test]
    fn test_decode_shift_jis_ascii() {
        test_group!("CSVパーサー");
        test_case!("ASCII文字のShift-JISデコード", {
            assert_eq!(decode_shift_jis(b"hello"), "hello");
        });
    }

    #[test]
    fn test_group_csv_by_unko_no() {
        test_group!("CSVパーサー");
        test_case!("運行NOでグループ化", {
            let csv = "運行NO,名前\n1001,田中\n1002,佐藤\n1001,鈴木\n";
            let map = group_csv_by_unko_no(csv);
            assert_eq!(map.len(), 2);
            assert_eq!(map["1001"].len(), 2);
            assert_eq!(map["1002"].len(), 1);
        });
    }

    #[test]
    fn test_group_csv_by_unko_no_empty_lines() {
        test_group!("CSVパーサー");
        test_case!("空行を含むCSVのグループ化", {
            let csv = "header\ndata1\n\n\ndata2\n";
            let map = group_csv_by_unko_no(csv);
            assert_eq!(map.len(), 2);
        });
    }

    #[test]
    fn test_csv_header() {
        test_group!("CSVパーサー");
        test_case!("CSVヘッダー取得", {
            assert_eq!(csv_header("col1,col2\nrow1"), Some("col1,col2"));
            assert_eq!(csv_header("single line"), Some("single line"));
        });
    }

    #[test]
    fn test_csv_header_none() {
        test_group!("CSVパーサー");
        test_case!("空CSVのヘッダーはNone", {
            assert_eq!(csv_header(""), None);
        });
    }
}
