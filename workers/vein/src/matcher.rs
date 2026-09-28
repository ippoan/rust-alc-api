//! crates/alc-vein/src/matcher.rs の写し (段階 A 限定。段階 B で alc-vein の
//! no-default-features を直接使って消す、Refs #682)。中身は変えていない (合成の
//! 特徴量 `synth` とテストは写していない)。`identify` の `expect` 2 つは上限の検査と
//! mode 1 の組み立てで到達しない不変条件なので、そのまま残している。

use base64::{engine::general_purpose::STANDARD, Engine};
use vein_match::feature::MAX_HEX_CHARS;
use vein_match::{DecodeError, PLANE_LEN};
use vein_match_search::{create_template, fv_search_user, Library};

/// `fv_search_user` の level (照合の閾値)。DLL (`FV_SearchUser`) は 1..=5 の外を 2 として
/// 扱う (vein-match の `search_level`) ので、その既定の 2 を使う。オフライン照合 (wasm) も
/// 同じ値で呼ぶこと。
pub const SEARCH_LEVEL: i32 = 2;

/// テンプレートの組み立て mode。1 = AES のみ (`import_temp_b64` で読み戻せる。wasm と同じ)。
pub const TEMPLATE_MODE: u8 = 1;

/// 1:N の上限。`Library::new(n)` は `1 < n <= 500` (DLL と同じ)。分割して照合すると
/// 学習と MRU (検索順) が DLL と変わりうるので、超えたら分割せずにエラーにする。
pub const MAX_TEMPLATES: usize = 500;

/// 特徴量 (端末が `VEIN CHARA <hex>` で返したもの) を受け付けられない理由。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharaError {
    /// 空・先頭が "BDBD" でない (外側の層・base64 等)。
    UnsupportedFormat,
    /// 奇数長・16 進でない文字を含む。
    InvalidHex,
    /// 3000 文字を超える (DLL の FV_CharaMatch と同じ上限)。
    TooLong,
    /// 0xBDBD だが版・CheckNum・大きさが合わない。
    Invalid,
}

impl CharaError {
    /// API の `error` に載せる機械可読なコード。
    pub fn code(self) -> &'static str {
        match self {
            CharaError::UnsupportedFormat => "unsupported_chara_format",
            CharaError::InvalidHex => "invalid_chara_hex",
            CharaError::TooLong => "chara_too_long",
            CharaError::Invalid => "invalid_chara",
        }
    }

    /// API の `message` に載せる説明。
    pub fn message(self) -> &'static str {
        match self {
            CharaError::UnsupportedFormat => {
                "未対応の形式です (先頭が BDBD の 16 進の特徴量だけを受け付けます)"
            }
            CharaError::InvalidHex => "特徴量の 16 進が壊れています (奇数長か 16 進でない文字)",
            CharaError::TooLong => "特徴量が長すぎます (16 進で 3000 文字まで)",
            CharaError::Invalid => "特徴量の中身が壊れています (版・検査和・大きさが合わない)",
        }
    }
}

fn decode_error(e: DecodeError) -> CharaError {
    match e {
        DecodeError::TooLong => CharaError::TooLong,
        DecodeError::Unsupported => CharaError::UnsupportedFormat,
        DecodeError::InvalidHex | DecodeError::BufferTooSmall => CharaError::InvalidHex,
    }
}

/// 16 進の特徴量をバイト列にし、`vein_match::load` が読めることまで確かめる。
/// 16 進の扱いは vein-match の `decode_hex` (wasm と同じ) に任せる。
pub fn decode_chara(hex: &str) -> Result<Vec<u8>, CharaError> {
    let mut buf = [0u8; MAX_HEX_CHARS / 2];
    let n = vein_match::decode_hex(hex.as_bytes(), &mut buf).map_err(decode_error)?;
    let mut plane = [0u8; PLANE_LEN];
    // "BDBD" で始まる 16 進なので外側の層 (Unsupported) には当たらず、失敗は版・CheckNum 等だけ
    vein_match::load(&buf[..n], n as u32, &mut plane).map_err(|_| CharaError::Invalid)?;
    Ok(buf[..n].to_vec())
}

/// 登録用の特徴量 (1..=6 件) から 1 件のテンプレート (base64) を組み立てる
/// (`FV_CreateVeinTemp` 相当)。件数が範囲外なら vein-match の戻り値 (-3) を返す。
pub fn enroll_template(charas: &[Vec<u8>]) -> Result<String, i32> {
    let refs: Vec<&[u8]> = charas.iter().map(Vec::as_slice).collect();
    create_template(&refs, 0, 0, TEMPLATE_MODE, &[]).map(|t| STANDARD.encode(t))
}

/// 1:N で当たった利用者。
#[derive(Debug, PartialEq, Eq)]
pub struct Hit {
    /// 渡したテンプレートの並びでの位置。
    pub index: usize,
    /// 学習後のテンプレート (base64)。
    pub learned: String,
}

/// [`identify`] の結果。
#[derive(Debug, PartialEq, Eq)]
pub struct Identify {
    pub hit: Option<Hit>,
    /// 読み込めずに照合から外したテンプレートの位置 (壊れた行を呼び出し側が記録する)。
    pub unreadable: Vec<usize>,
}

/// 人数が [`MAX_TEMPLATES`] を超えている。
#[derive(Debug, PartialEq, Eq)]
pub struct TooManyTemplates(pub usize);

/// `n` 人の登録で 1:N を組めるか。照合 ([`identify`]) と登録 (PUT) の両方がこれで判定する
/// (登録で上限を超えさせると、その瞬間にテナント全員の照合が止まるため)。
pub fn check_capacity(n: usize) -> Result<(), TooManyTemplates> {
    if n > MAX_TEMPLATES {
        return Err(TooManyTemplates(n));
    }
    Ok(())
}

/// テナントの全テンプレートで `Library` を組み、`chara` (検査済みの 0xBDBD 構造体) を
/// 1:N で照合する。当たれば学習 (`t8` = 今の unix 秒の下位 8 ビット) し、学習後の
/// テンプレートを取り出す。0〜1 人でも動くよう `Library` は `max(件数, 2)` 人ぶんで組む。
pub fn identify(templates: &[&str], chara: &[u8], t8: u8) -> Result<Identify, TooManyTemplates> {
    check_capacity(templates.len())?;
    let mut lib = Library::new(templates.len().max(2)).expect("2..=500 人は Library::new の範囲内");
    let mut unreadable = Vec::new();
    for (i, t) in templates.iter().enumerate() {
        if lib.import_temp_b64(i as u32 + 1, t) != 0 {
            unreadable.push(i);
        }
    }
    let (r, _) = fv_search_user(&mut lib, chara, SEARCH_LEVEL, Some(t8));
    let hit = (r > 0).then(|| {
        let index = (r - 1) as usize;
        // 当たった利用者には記録があり (get_enroll_plain が None にならない)、mode 1 の
        // 組み立ては失敗しない (EncodeUnsupported は mode の bit1/bit2 だけ) ので取り出せる
        let learned = lib
            .get_enroll_template(index, TEMPLATE_MODE, &[])
            .and_then(Result::ok)
            .expect("当たった利用者の学習後テンプレートは取り出せる");
        Hit {
            index,
            learned: STANDARD.encode(learned),
        }
    });
    Ok(Identify { hit, unreadable })
}
