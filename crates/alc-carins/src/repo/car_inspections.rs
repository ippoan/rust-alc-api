use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::tenant::TenantConn;

pub use alc_core::repository::car_inspections::*;

fn get_str<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn strip_spaces(s: &str) -> String {
    s.replace([' ', '\u{3000}'], "")
}

pub struct PgCarInspectionRepository {
    pool: PgPool,
}

impl PgCarInspectionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CarInspectionRepository for PgCarInspectionRepository {
    async fn list_current(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rows = sqlx::query_as::<_, (serde_json::Value,)>(
            r#"
            SELECT to_jsonb(sub) FROM (
                SELECT DISTINCT ON (ci."CarId")
                    ci.*,
                    (SELECT uuid::text FROM car_inspection_files_b
                     WHERE tenant_id = ci.tenant_id
                       AND "ElectCertMgNo" = ci."ElectCertMgNo"
                       AND "GrantdateE" = ci."GrantdateE"
                       AND "GrantdateY" = ci."GrantdateY"
                       AND "GrantdateM" = ci."GrantdateM"
                       AND "GrantdateD" = ci."GrantdateD"
                       AND type = 'application/pdf'
                       AND deleted_at IS NULL
                     ORDER BY created_at DESC LIMIT 1) as "pdfUuid",
                    (SELECT uuid::text FROM car_inspection_files_a
                     WHERE tenant_id = ci.tenant_id
                       AND "ElectCertMgNo" = ci."ElectCertMgNo"
                       AND "GrantdateE" = ci."GrantdateE"
                       AND "GrantdateY" = ci."GrantdateY"
                       AND "GrantdateM" = ci."GrantdateM"
                       AND "GrantdateD" = ci."GrantdateD"
                       AND type = 'application/json'
                       AND deleted_at IS NULL
                     ORDER BY created_at DESC LIMIT 1) as "jsonUuid"
                FROM car_inspection ci
                ORDER BY ci."CarId",
                         ci."TwodimensionCodeInfoValidPeriodExpirdate" DESC,
                         ci.created_at DESC
            ) sub
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn list_expired(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rows = sqlx::query_as::<_, (serde_json::Value,)>(
            r#"
            SELECT to_jsonb(ci)
            FROM car_inspection ci
            WHERE "TwodimensionCodeInfoValidPeriodExpirdate" <= to_char(CURRENT_DATE + INTERVAL '30 days', 'YYMMDD')
            ORDER BY "TwodimensionCodeInfoValidPeriodExpirdate" ASC
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn list_renew(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rows = sqlx::query_as::<_, (serde_json::Value,)>(
            r#"
            SELECT to_jsonb(ci)
            FROM car_inspection ci
            WHERE "TwodimensionCodeInfoValidPeriodExpirdate" >= to_char(CURRENT_DATE, 'YYMMDD')
              AND "TwodimensionCodeInfoValidPeriodExpirdate" <= to_char(CURRENT_DATE + INTERVAL '60 days', 'YYMMDD')
            ORDER BY "TwodimensionCodeInfoValidPeriodExpirdate" ASC
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn get_by_id(
        &self,
        tenant_id: Uuid,
        id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let row = sqlx::query_as::<_, (serde_json::Value,)>(
            "SELECT to_jsonb(ci) FROM car_inspection ci WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(row.map(|r| r.0))
    }

    async fn vehicle_categories(&self, tenant_id: Uuid) -> Result<VehicleCategories, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, VehicleCategories>(
            r#"SELECT
                COALESCE(ARRAY(SELECT DISTINCT "CarKind" FROM alc_api.car_inspection WHERE "CarKind" != '' ORDER BY "CarKind"), '{}') AS car_kinds,
                COALESCE(ARRAY(SELECT DISTINCT "Use" FROM alc_api.car_inspection WHERE "Use" != '' ORDER BY "Use"), '{}') AS uses,
                COALESCE(ARRAY(SELECT DISTINCT "CarShape" FROM alc_api.car_inspection WHERE "CarShape" != '' ORDER BY "CarShape"), '{}') AS car_shapes,
                COALESCE(ARRAY(SELECT DISTINCT "PrivateBusiness" FROM alc_api.car_inspection WHERE "PrivateBusiness" != '' ORDER BY "PrivateBusiness"), '{}') AS private_businesses
            "#,
        )
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_current_files(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CarInspectionFile>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CarInspectionFile>(
            r#"
            SELECT cif.*
            FROM car_inspection_files_a cif
            INNER JOIN car_inspection ci ON
                cif."ElectCertMgNo" = ci."ElectCertMgNo"
                AND cif."GrantdateE" = ci."GrantdateE"
                AND cif."GrantdateY" = ci."GrantdateY"
                AND cif."GrantdateM" = ci."GrantdateM"
                AND cif."GrantdateD" = ci."GrantdateD"
            WHERE cif.deleted_at IS NULL
              AND ci."TwodimensionCodeInfoValidPeriodExpirdate" >= to_char(CURRENT_DATE, 'YYMMDD')
            ORDER BY cif.created_at DESC
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn upsert_from_json(
        &self,
        tenant_id: Uuid,
        cert_info: &Value,
        cert_info_import_file_version: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let grantdate_e = strip_spaces(get_str(cert_info, "GrantdateE"));
        let grantdate_y = strip_spaces(get_str(cert_info, "GrantdateY"));
        let grantdate_m = strip_spaces(get_str(cert_info, "GrantdateM"));
        let grantdate_d = strip_spaces(get_str(cert_info, "GrantdateD"));

        sqlx::query(
            r#"
            INSERT INTO car_inspection (
                tenant_id,
                "CertInfoImportFileVersion", "Acceptoutputno", "FormType", "ElectCertMgNo", "CarId",
                "ElectCertPublishdateE", "ElectCertPublishdateY", "ElectCertPublishdateM", "ElectCertPublishdateD",
                "GrantdateE", "GrantdateY", "GrantdateM", "GrantdateD",
                "TranspotationBureauchiefName", "EntryNoCarNo",
                "ReggrantdateE", "ReggrantdateY", "ReggrantdateM", "ReggrantdateD",
                "FirstregistdateE", "FirstregistdateY", "FirstregistdateM",
                "CarName", "CarNameCode", "CarNo", "Model", "EngineModel",
                "OwnernameLowLevelChar", "OwnernameHighLevelChar", "OwnerAddressChar", "OwnerAddressNumValue", "OwnerAddressCode",
                "UsernameLowLevelChar", "UsernameHighLevelChar", "UserAddressChar", "UserAddressNumValue", "UserAddressCode",
                "UseheadqrterChar", "UseheadqrterNumValue", "UseheadqrterCode",
                "CarKind", "Use", "PrivateBusiness", "CarShape", "CarShapeCode",
                "NoteCap", "Cap", "NoteMaxloadage", "Maxloadage",
                "NoteCarWgt", "CarWgt", "NoteCarTotalWgt", "CarTotalWgt",
                "NoteLength", "Length", "NoteWidth", "Width", "NoteHeight", "Height",
                "FfAxWgt", "FrAxWgt", "RfAxWgt", "RrAxWgt",
                "Displacement", "FuelClass", "ModelSpecifyNo", "ClassifyAroundNo",
                "ValidPeriodExpirdateE", "ValidPeriodExpirdateY", "ValidPeriodExpirdateM", "ValidPeriodExpirdateD",
                "NoteInfo",
                "TwodimensionCodeInfoEntryNoCarNo", "TwodimensionCodeInfoCarNo", "TwodimensionCodeInfoValidPeriodExpirdate",
                "TwodimensionCodeInfoModel", "TwodimensionCodeInfoModelSpecifyNoClassifyAroundNo",
                "TwodimensionCodeInfoCharInfo", "TwodimensionCodeInfoEngineModel", "TwodimensionCodeInfoCarNoStampPlace",
                "TwodimensionCodeInfoFirstregistdate",
                "TwodimensionCodeInfoFfAxWgt", "TwodimensionCodeInfoFrAxWgt", "TwodimensionCodeInfoRfAxWgt", "TwodimensionCodeInfoRrAxWgt",
                "TwodimensionCodeInfoNoiseReg", "TwodimensionCodeInfoNearNoiseReg", "TwodimensionCodeInfoDriveMethod",
                "TwodimensionCodeInfoOpacimeterMeasCar", "TwodimensionCodeInfoNoxPmMeasMode",
                "TwodimensionCodeInfoNoxValue", "TwodimensionCodeInfoPmValue",
                "TwodimensionCodeInfoSafeStdDate", "TwodimensionCodeInfoFuelClassCode",
                "RegistCarLightCar"
            ) VALUES (
                $1,
                $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                $21, $22, $23, $24, $25, $26, $27, $28, $29, $30,
                $31, $32, $33, $34, $35, $36, $37, $38, $39, $40,
                $41, $42, $43, $44, $45, $46, $47, $48, $49, $50,
                $51, $52, $53, $54, $55, $56, $57, $58, $59, $60,
                $61, $62, $63, $64, $65, $66, $67, $68, $69, $70,
                $71, $72, $73, $74, $75, $76, $77, $78, $79, $80,
                $81, $82, $83, $84, $85, $86, $87, $88, $89, $90,
                $91, $92, $93, $94, $95, $96
            )
            ON CONFLICT (tenant_id, "ElectCertMgNo", "GrantdateE", "GrantdateY", "GrantdateM", "GrantdateD")
            DO UPDATE SET
                "CertInfoImportFileVersion" = EXCLUDED."CertInfoImportFileVersion",
                "Acceptoutputno" = EXCLUDED."Acceptoutputno",
                "FormType" = EXCLUDED."FormType",
                "CarId" = EXCLUDED."CarId",
                "ElectCertPublishdateE" = EXCLUDED."ElectCertPublishdateE",
                "ElectCertPublishdateY" = EXCLUDED."ElectCertPublishdateY",
                "ElectCertPublishdateM" = EXCLUDED."ElectCertPublishdateM",
                "ElectCertPublishdateD" = EXCLUDED."ElectCertPublishdateD",
                "TranspotationBureauchiefName" = EXCLUDED."TranspotationBureauchiefName",
                "EntryNoCarNo" = EXCLUDED."EntryNoCarNo",
                "ReggrantdateE" = EXCLUDED."ReggrantdateE",
                "ReggrantdateY" = EXCLUDED."ReggrantdateY",
                "ReggrantdateM" = EXCLUDED."ReggrantdateM",
                "ReggrantdateD" = EXCLUDED."ReggrantdateD",
                "FirstregistdateE" = EXCLUDED."FirstregistdateE",
                "FirstregistdateY" = EXCLUDED."FirstregistdateY",
                "FirstregistdateM" = EXCLUDED."FirstregistdateM",
                "CarName" = EXCLUDED."CarName",
                "CarNameCode" = EXCLUDED."CarNameCode",
                "CarNo" = EXCLUDED."CarNo",
                "Model" = EXCLUDED."Model",
                "EngineModel" = EXCLUDED."EngineModel",
                "OwnernameLowLevelChar" = EXCLUDED."OwnernameLowLevelChar",
                "OwnernameHighLevelChar" = EXCLUDED."OwnernameHighLevelChar",
                "OwnerAddressChar" = EXCLUDED."OwnerAddressChar",
                "OwnerAddressNumValue" = EXCLUDED."OwnerAddressNumValue",
                "OwnerAddressCode" = EXCLUDED."OwnerAddressCode",
                "UsernameLowLevelChar" = EXCLUDED."UsernameLowLevelChar",
                "UsernameHighLevelChar" = EXCLUDED."UsernameHighLevelChar",
                "UserAddressChar" = EXCLUDED."UserAddressChar",
                "UserAddressNumValue" = EXCLUDED."UserAddressNumValue",
                "UserAddressCode" = EXCLUDED."UserAddressCode",
                "UseheadqrterChar" = EXCLUDED."UseheadqrterChar",
                "UseheadqrterNumValue" = EXCLUDED."UseheadqrterNumValue",
                "UseheadqrterCode" = EXCLUDED."UseheadqrterCode",
                "CarKind" = EXCLUDED."CarKind",
                "Use" = EXCLUDED."Use",
                "PrivateBusiness" = EXCLUDED."PrivateBusiness",
                "CarShape" = EXCLUDED."CarShape",
                "CarShapeCode" = EXCLUDED."CarShapeCode",
                "NoteCap" = EXCLUDED."NoteCap",
                "Cap" = EXCLUDED."Cap",
                "NoteMaxloadage" = EXCLUDED."NoteMaxloadage",
                "Maxloadage" = EXCLUDED."Maxloadage",
                "NoteCarWgt" = EXCLUDED."NoteCarWgt",
                "CarWgt" = EXCLUDED."CarWgt",
                "NoteCarTotalWgt" = EXCLUDED."NoteCarTotalWgt",
                "CarTotalWgt" = EXCLUDED."CarTotalWgt",
                "NoteLength" = EXCLUDED."NoteLength",
                "Length" = EXCLUDED."Length",
                "NoteWidth" = EXCLUDED."NoteWidth",
                "Width" = EXCLUDED."Width",
                "NoteHeight" = EXCLUDED."NoteHeight",
                "Height" = EXCLUDED."Height",
                "FfAxWgt" = EXCLUDED."FfAxWgt",
                "FrAxWgt" = EXCLUDED."FrAxWgt",
                "RfAxWgt" = EXCLUDED."RfAxWgt",
                "RrAxWgt" = EXCLUDED."RrAxWgt",
                "Displacement" = EXCLUDED."Displacement",
                "FuelClass" = EXCLUDED."FuelClass",
                "ModelSpecifyNo" = EXCLUDED."ModelSpecifyNo",
                "ClassifyAroundNo" = EXCLUDED."ClassifyAroundNo",
                "ValidPeriodExpirdateE" = EXCLUDED."ValidPeriodExpirdateE",
                "ValidPeriodExpirdateY" = EXCLUDED."ValidPeriodExpirdateY",
                "ValidPeriodExpirdateM" = EXCLUDED."ValidPeriodExpirdateM",
                "ValidPeriodExpirdateD" = EXCLUDED."ValidPeriodExpirdateD",
                "NoteInfo" = EXCLUDED."NoteInfo",
                "TwodimensionCodeInfoEntryNoCarNo" = EXCLUDED."TwodimensionCodeInfoEntryNoCarNo",
                "TwodimensionCodeInfoCarNo" = EXCLUDED."TwodimensionCodeInfoCarNo",
                "TwodimensionCodeInfoValidPeriodExpirdate" = EXCLUDED."TwodimensionCodeInfoValidPeriodExpirdate",
                "TwodimensionCodeInfoModel" = EXCLUDED."TwodimensionCodeInfoModel",
                "TwodimensionCodeInfoModelSpecifyNoClassifyAroundNo" = EXCLUDED."TwodimensionCodeInfoModelSpecifyNoClassifyAroundNo",
                "TwodimensionCodeInfoCharInfo" = EXCLUDED."TwodimensionCodeInfoCharInfo",
                "TwodimensionCodeInfoEngineModel" = EXCLUDED."TwodimensionCodeInfoEngineModel",
                "TwodimensionCodeInfoCarNoStampPlace" = EXCLUDED."TwodimensionCodeInfoCarNoStampPlace",
                "TwodimensionCodeInfoFirstregistdate" = EXCLUDED."TwodimensionCodeInfoFirstregistdate",
                "TwodimensionCodeInfoFfAxWgt" = EXCLUDED."TwodimensionCodeInfoFfAxWgt",
                "TwodimensionCodeInfoFrAxWgt" = EXCLUDED."TwodimensionCodeInfoFrAxWgt",
                "TwodimensionCodeInfoRfAxWgt" = EXCLUDED."TwodimensionCodeInfoRfAxWgt",
                "TwodimensionCodeInfoRrAxWgt" = EXCLUDED."TwodimensionCodeInfoRrAxWgt",
                "TwodimensionCodeInfoNoiseReg" = EXCLUDED."TwodimensionCodeInfoNoiseReg",
                "TwodimensionCodeInfoNearNoiseReg" = EXCLUDED."TwodimensionCodeInfoNearNoiseReg",
                "TwodimensionCodeInfoDriveMethod" = EXCLUDED."TwodimensionCodeInfoDriveMethod",
                "TwodimensionCodeInfoOpacimeterMeasCar" = EXCLUDED."TwodimensionCodeInfoOpacimeterMeasCar",
                "TwodimensionCodeInfoNoxPmMeasMode" = EXCLUDED."TwodimensionCodeInfoNoxPmMeasMode",
                "TwodimensionCodeInfoNoxValue" = EXCLUDED."TwodimensionCodeInfoNoxValue",
                "TwodimensionCodeInfoPmValue" = EXCLUDED."TwodimensionCodeInfoPmValue",
                "TwodimensionCodeInfoSafeStdDate" = EXCLUDED."TwodimensionCodeInfoSafeStdDate",
                "TwodimensionCodeInfoFuelClassCode" = EXCLUDED."TwodimensionCodeInfoFuelClassCode",
                "RegistCarLightCar" = EXCLUDED."RegistCarLightCar",
                modified_at = NOW()
            "#,
        )
        .bind(tenant_id)                                                              // $1
        .bind(cert_info_import_file_version)                                          // $2
        .bind(get_str(cert_info, "Acceptoutputno"))                                   // $3
        .bind(get_str(cert_info, "FormType"))                                         // $4
        .bind(get_str(cert_info, "ElectCertMgNo"))                                    // $5
        .bind(get_str(cert_info, "CarId"))                                            // $6
        .bind(get_str(cert_info, "ElectCertPublishdateE"))                            // $7
        .bind(get_str(cert_info, "ElectCertPublishdateY"))                            // $8
        .bind(get_str(cert_info, "ElectCertPublishdateM"))                            // $9
        .bind(get_str(cert_info, "ElectCertPublishdateD"))                            // $10
        .bind(&grantdate_e)                                                           // $11
        .bind(&grantdate_y)                                                           // $12
        .bind(&grantdate_m)                                                           // $13
        .bind(&grantdate_d)                                                           // $14
        .bind(get_str(cert_info, "TranspotationBureauchiefName"))                     // $15
        .bind(get_str(cert_info, "EntryNoCarNo"))                                     // $16
        .bind(get_str(cert_info, "ReggrantdateE"))                                    // $17
        .bind(get_str(cert_info, "ReggrantdateY"))                                    // $18
        .bind(get_str(cert_info, "ReggrantdateM"))                                    // $19
        .bind(get_str(cert_info, "ReggrantdateD"))                                    // $20
        .bind(get_str(cert_info, "FirstregistdateE"))                                 // $21
        .bind(get_str(cert_info, "FirstregistdateY"))                                 // $22
        .bind(get_str(cert_info, "FirstregistdateM"))                                 // $23
        .bind(get_str(cert_info, "CarName"))                                          // $24
        .bind(get_str(cert_info, "CarNameCode"))                                      // $25
        .bind(get_str(cert_info, "CarNo"))                                            // $26
        .bind(get_str(cert_info, "Model"))                                            // $27
        .bind(get_str(cert_info, "EngineModel"))                                      // $28
        .bind(get_str(cert_info, "OwnernameLowLevelChar"))                            // $29
        .bind(get_str(cert_info, "OwnernameHighLevelChar"))                           // $30
        .bind(get_str(cert_info, "OwnerAddressChar"))                                 // $31
        .bind(get_str(cert_info, "OwnerAddressNumValue"))                             // $32
        .bind(get_str(cert_info, "OwnerAddressCode"))                                 // $33
        .bind(get_str(cert_info, "UsernameLowLevelChar"))                             // $34
        .bind(get_str(cert_info, "UsernameHighLevelChar"))                            // $35
        .bind(get_str(cert_info, "UserAddressChar"))                                  // $36
        .bind(get_str(cert_info, "UserAddressNumValue"))                              // $37
        .bind(get_str(cert_info, "UserAddressCode"))                                  // $38
        .bind(get_str(cert_info, "UseheadqrterChar"))                                 // $39
        .bind(get_str(cert_info, "UseheadqrterNumValue"))                             // $40
        .bind(get_str(cert_info, "UseheadqrterCode"))                                 // $41
        .bind(get_str(cert_info, "CarKind"))                                          // $42
        .bind(get_str(cert_info, "Use"))                                              // $43
        .bind(get_str(cert_info, "PrivateBusiness"))                                  // $44
        .bind(get_str(cert_info, "CarShape"))                                         // $45
        .bind(get_str(cert_info, "CarShapeCode"))                                     // $46
        .bind(get_str(cert_info, "NoteCap"))                                          // $47
        .bind(get_str(cert_info, "Cap"))                                              // $48
        .bind(get_str(cert_info, "NoteMaxloadage"))                                   // $49
        .bind(get_str(cert_info, "Maxloadage"))                                       // $50
        .bind(get_str(cert_info, "NoteCarWgt"))                                       // $51
        .bind(get_str(cert_info, "CarWgt"))                                           // $52
        .bind(get_str(cert_info, "NoteCarTotalWgt"))                                  // $53
        .bind(get_str(cert_info, "CarTotalWgt"))                                      // $54
        .bind(get_str(cert_info, "NoteLength"))                                       // $55
        .bind(get_str(cert_info, "Length"))                                            // $56
        .bind(get_str(cert_info, "NoteWidth"))                                        // $57
        .bind(get_str(cert_info, "Width"))                                            // $58
        .bind(get_str(cert_info, "NoteHeight"))                                       // $59
        .bind(get_str(cert_info, "Height"))                                           // $60
        .bind(get_str(cert_info, "FfAxWgt"))                                          // $61
        .bind(get_str(cert_info, "FrAxWgt"))                                          // $62
        .bind(get_str(cert_info, "RfAxWgt"))                                          // $63
        .bind(get_str(cert_info, "RrAxWgt"))                                          // $64
        .bind(get_str(cert_info, "Displacement"))                                     // $65
        .bind(get_str(cert_info, "FuelClass"))                                        // $66
        .bind(get_str(cert_info, "ModelSpecifyNo"))                                   // $67
        .bind(get_str(cert_info, "ClassifyAroundNo"))                                 // $68
        .bind(get_str(cert_info, "ValidPeriodExpirdateE"))                             // $69
        .bind(get_str(cert_info, "ValidPeriodExpirdateY"))                             // $70
        .bind(get_str(cert_info, "ValidPeriodExpirdateM"))                             // $71
        .bind(get_str(cert_info, "ValidPeriodExpirdateD"))                             // $72
        .bind(get_str(cert_info, "NoteInfo"))                                         // $73
        .bind(get_str(cert_info, "TwodimensionCodeInfoEntryNoCarNo"))                 // $74
        .bind(get_str(cert_info, "TwodimensionCodeInfoCarNo"))                        // $75
        .bind(get_str(cert_info, "TwodimensionCodeInfoValidPeriodExpirdate"))          // $76
        .bind(get_str(cert_info, "TwodimensionCodeInfoModel"))                        // $77
        .bind(get_str(cert_info, "TwodimensionCodeInfoModelSpecifyNoClassifyAroundNo")) // $78
        .bind(get_str(cert_info, "TwodimensionCodeInfoCharInfo"))                     // $79
        .bind(get_str(cert_info, "TwodimensionCodeInfoEngineModel"))                  // $80
        .bind(get_str(cert_info, "TwodimensionCodeInfoCarNoStampPlace"))              // $81
        .bind(get_str(cert_info, "TwodimensionCodeInfoFirstregistdate"))              // $82
        .bind(get_str(cert_info, "TwodimensionCodeInfoFfAxWgt"))                      // $83
        .bind(get_str(cert_info, "TwodimensionCodeInfoFrAxWgt"))                      // $84
        .bind(get_str(cert_info, "TwodimensionCodeInfoRfAxWgt"))                      // $85
        .bind(get_str(cert_info, "TwodimensionCodeInfoRrAxWgt"))                      // $86
        .bind(get_str(cert_info, "TwodimensionCodeInfoNoiseReg"))                     // $87
        .bind(get_str(cert_info, "TwodimensionCodeInfoNearNoiseReg"))                 // $88
        .bind(get_str(cert_info, "TwodimensionCodeInfoDriveMethod"))                  // $89
        .bind(get_str(cert_info, "TwodimensionCodeInfoOpacimeterMeasCar"))            // $90
        .bind(get_str(cert_info, "TwodimensionCodeInfoNoxPmMeasMode"))               // $91
        .bind(get_str(cert_info, "TwodimensionCodeInfoNoxValue"))                     // $92
        .bind(get_str(cert_info, "TwodimensionCodeInfoPmValue"))                      // $93
        .bind(get_str(cert_info, "TwodimensionCodeInfoSafeStdDate"))                  // $94
        .bind(get_str(cert_info, "TwodimensionCodeInfoFuelClassCode"))                // $95
        .bind(get_str(cert_info, "RegistCarLightCar"))                                // $96
        .execute(&mut *tc.conn)
        .await?;

        Ok(())
    }

    async fn create_file_link(&self, params: &CreateFileLinkParams<'_>) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &params.tenant_id.to_string()).await?;

        let table = if params.file_type == "application/pdf" {
            "car_inspection_files_b"
        } else {
            "car_inspection_files_a"
        };

        let sql = format!(
            r#"
            INSERT INTO {table} (uuid, tenant_id, type, "ElectCertMgNo", "GrantdateE", "GrantdateY", "GrantdateM", "GrantdateD")
            VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (uuid) DO UPDATE SET modified_at = NOW()
            "#,
        );

        sqlx::query(&sql)
            .bind(params.file_uuid)
            .bind(params.tenant_id)
            .bind(params.file_type)
            .bind(params.elect_cert_mg_no)
            .bind(params.grantdate_e)
            .bind(params.grantdate_y)
            .bind(params.grantdate_m)
            .bind(params.grantdate_d)
            .execute(&mut *tc.conn)
            .await?;

        Ok(())
    }

    async fn find_pending_pdf(
        &self,
        tenant_id: Uuid,
        elect_cert_mg_no: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let row = sqlx::query_as::<_, (String,)>(
            r#"SELECT file_uuid::text FROM pending_car_inspection_pdfs WHERE "ElectCertMgNo" = $1"#,
        )
        .bind(elect_cert_mg_no)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(row.map(|r| r.0))
    }

    async fn delete_pending_pdf(
        &self,
        tenant_id: Uuid,
        elect_cert_mg_no: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(r#"DELETE FROM pending_car_inspection_pdfs WHERE "ElectCertMgNo" = $1"#)
            .bind(elect_cert_mg_no)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn upsert_pending_pdf(
        &self,
        params: &CreateFileLinkParams<'_>,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &params.tenant_id.to_string()).await?;
        sqlx::query(
            r#"
            INSERT INTO pending_car_inspection_pdfs (tenant_id, file_uuid, "ElectCertMgNo", "GrantdateE", "GrantdateY", "GrantdateM", "GrantdateD")
            VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7)
            ON CONFLICT (tenant_id, "ElectCertMgNo")
            DO UPDATE SET file_uuid = EXCLUDED.file_uuid,
                          "GrantdateE" = EXCLUDED."GrantdateE",
                          "GrantdateY" = EXCLUDED."GrantdateY",
                          "GrantdateM" = EXCLUDED."GrantdateM",
                          "GrantdateD" = EXCLUDED."GrantdateD",
                          created_at = NOW()
            "#,
        )
        .bind(params.tenant_id)
        .bind(params.file_uuid)
        .bind(params.elect_cert_mg_no)
        .bind(params.grantdate_e)
        .bind(params.grantdate_y)
        .bind(params.grantdate_m)
        .bind(params.grantdate_d)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn json_file_exists(
        &self,
        tenant_id: Uuid,
        elect_cert_mg_no: &str,
        grantdate_e: &str,
        grantdate_y: &str,
        grantdate_m: &str,
        grantdate_d: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM car_inspection_files_a
                WHERE "ElectCertMgNo" = $1
                  AND "GrantdateE" = $2 AND "GrantdateY" = $3
                  AND "GrantdateM" = $4 AND "GrantdateD" = $5
                  AND type = 'application/json'
                  AND deleted_at IS NULL
            )
            "#,
        )
        .bind(elect_cert_mg_no)
        .bind(grantdate_e)
        .bind(grantdate_y)
        .bind(grantdate_m)
        .bind(grantdate_d)
        .fetch_one(&mut *tc.conn)
        .await?;
        Ok(exists)
    }
}
