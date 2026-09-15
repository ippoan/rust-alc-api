//! 通常点呼 (運行者端末の測定) を点呼セッション・点呼記録に載せる。
//!
//! 運行者端末は管理者ログイン無しで動くため、点呼の段 (`submit_medical` →
//! `submit_alcohol` → …) を踏まない。代わりに **測定の保存と同じ transaction** の中で
//! セッションと記録を 1 組作る (Refs ippoan/alc-app#238, ippoan/alc-app-s3#135)。
//!
//! 呼び元は `alc-misc` の測定 repository。接続を自分で取らず `conn` を受け取るのは、
//! 測定の INSERT / UPDATE と同じ transaction に相乗りするため。

use sqlx::PgConnection;

use alc_core::models::Measurement;

use crate::models::TenkoSession;
use crate::repo::tenko_sessions::insert_record;
use crate::tenko_sessions::{record_payload, status_for_result};

/// 通常点呼の種別の既定値。運行者端末で始業 / 終業を選ぶと `pre_operation` /
/// `post_operation` が渡ってくる (2026-09 のオーナー判断で migration 140 の方針を改めた)。
const TENKO_TYPE_NORMAL: &str = "normal";

/// 点呼セッション・点呼記録の点呼方法。運行管理者の一覧・CSV にこの文字列が出る。
/// 種別 (業務前 / 業務後) とは軸が違うので、始業 / 終業を選んでもこのまま。
const TENKO_METHOD_NORMAL: &str = "通常点呼";

/// 完了した通常点呼の測定から、点呼セッションと点呼記録を 1 組作る。
///
/// * 結果が `error` / 無し → `Ok(None)` (測定だけ残す。エラーにしない)
/// * `tenko_type` が `None` → `normal` (値の検査は handler で済ませてある)
/// * 既に記録済み (同じ測定、または同じ乗務員・同じ測定時刻) → `Ok(None)`
///   — migration 141 の部分 unique に `ON CONFLICT DO NOTHING` で任せる
pub async fn record(
    conn: &mut PgConnection,
    m: &Measurement,
    tenko_type: Option<&str>,
) -> Result<Option<TenkoSession>, sqlx::Error> {
    let Some((status, cancel_reason)) = status_for_result(m.result.as_deref()) else {
        return Ok(None);
    };

    let session = sqlx::query_as::<_, TenkoSession>(
        r#"
        INSERT INTO tenko_sessions (
            tenant_id, employee_id, schedule_id, tenko_type, status,
            measurement_id, alcohol_result, alcohol_value, alcohol_tested_at,
            alcohol_face_photo_url,
            temperature, systolic, diastolic, pulse, medical_measured_at,
            medical_manual_input,
            responsible_manager_name, cancel_reason,
            started_at, completed_at, tenko_method
        )
        VALUES (
            $1, $2, NULL, $3, $4,
            $5, $6, $7, $8,
            $9,
            $10, $11, $12, $13, $14,
            $15,
            NULL, $16,
            $17, NOW(), $18
        )
        ON CONFLICT DO NOTHING
        RETURNING *
        "#,
    )
    .bind(m.tenant_id)
    .bind(m.employee_id)
    .bind(tenko_type.unwrap_or(TENKO_TYPE_NORMAL))
    .bind(status)
    .bind(m.id)
    .bind(&m.result)
    .bind(m.alcohol_level)
    .bind(m.measured_at)
    .bind(&m.face_photo_url)
    .bind(m.temperature)
    .bind(m.systolic)
    .bind(m.diastolic)
    .bind(m.pulse)
    .bind(m.medical_measured_at)
    .bind(m.medical_manual_input)
    .bind(cancel_reason)
    .bind(m.measured_at)
    .bind(TENKO_METHOD_NORMAL)
    .fetch_optional(&mut *conn)
    .await?;

    // 既に記録済み — 部分 unique が弾いた (同じ測定への再送、またはオフライン保存で
    // 測定がもう 1 件できた経路)。二重に記録しない。
    let Some(session) = session else {
        return Ok(None);
    };

    // 乗務員の名前は同じ transaction の中で引く
    // (repository の `get_employee_name` は自前で接続を取るので使えない)。
    let employee_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM employees WHERE id = $1")
            .bind(m.employee_id)
            .fetch_optional(&mut *conn)
            .await?;

    let (record_data, record_hash) =
        record_payload(&session).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;

    insert_record(
        conn,
        m.tenant_id,
        &session,
        &employee_name.unwrap_or_default(),
        &None,
        &record_data,
        &record_hash,
        TENKO_METHOD_NORMAL,
    )
    .await?;

    Ok(Some(session))
}
