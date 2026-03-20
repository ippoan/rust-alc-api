-- carins テーブル群（rust-logi から移動）
-- 車検証管理、ファイル管理、NFC タグ、SSO 設定

-- files テーブル
CREATE TABLE IF NOT EXISTS alc_api.files (
    uuid UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    filename TEXT NOT NULL,
    type TEXT NOT NULL,
    blob TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    s3_key TEXT,
    storage_class TEXT DEFAULT 'STANDARD',
    last_accessed_at TIMESTAMPTZ,
    access_count_weekly INTEGER NOT NULL DEFAULT 0,
    week_started_at TIMESTAMPTZ,
    access_count_total INTEGER NOT NULL DEFAULT 0,
    promoted_to_standard_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_files_organization_id ON alc_api.files (tenant_id);
CREATE INDEX IF NOT EXISTS idx_files_org_deleted ON alc_api.files (tenant_id, deleted_at) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_files_s3_key ON alc_api.files (s3_key) WHERE s3_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_files_last_accessed ON alc_api.files (last_accessed_at);

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'files' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.files ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'files' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.files FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'files' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.files
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.files TO alc_api_app;

-- files_append テーブル
CREATE TABLE IF NOT EXISTS alc_api.files_append (
    appendname TEXT PRIMARY KEY,
    tenant_id UUID NOT NULL,
    file_uuid UUID NOT NULL REFERENCES alc_api.files(uuid),
    appendtype TEXT NOT NULL,
    type TEXT NOT NULL,
    page INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_files_append_organization_id ON alc_api.files_append (tenant_id);
CREATE INDEX IF NOT EXISTS idx_files_append_file_uuid ON alc_api.files_append (file_uuid);

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'files_append' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.files_append ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'files_append' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.files_append FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'files_append' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.files_append
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.files_append TO alc_api_app;

-- file_access_logs テーブル
CREATE TABLE IF NOT EXISTS alc_api.file_access_logs (
    id BIGSERIAL PRIMARY KEY,
    file_uuid UUID NOT NULL REFERENCES alc_api.files(uuid) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    accessed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    storage_class_at_access TEXT
);

CREATE INDEX IF NOT EXISTS idx_file_access_logs_file_uuid ON alc_api.file_access_logs (file_uuid);
CREATE INDEX IF NOT EXISTS idx_file_access_logs_accessed_at ON alc_api.file_access_logs (accessed_at);
CREATE INDEX IF NOT EXISTS idx_file_access_logs_org_accessed ON alc_api.file_access_logs (tenant_id, accessed_at);

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'file_access_logs' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.file_access_logs ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'file_access_logs' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.file_access_logs FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'file_access_logs' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.file_access_logs
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.file_access_logs TO alc_api_app;
GRANT USAGE, SELECT ON SEQUENCE alc_api.file_access_logs_id_seq TO alc_api_app;

-- car_inspection テーブル
CREATE TABLE IF NOT EXISTS alc_api.car_inspection (
    id SERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    "CertInfoImportFileVersion" TEXT NOT NULL,
    "Acceptoutputno" TEXT NOT NULL,
    "FormType" TEXT NOT NULL,
    "ElectCertMgNo" TEXT NOT NULL,
    "CarId" TEXT NOT NULL,
    "ElectCertPublishdateE" TEXT NOT NULL,
    "ElectCertPublishdateY" TEXT NOT NULL,
    "ElectCertPublishdateM" TEXT NOT NULL,
    "ElectCertPublishdateD" TEXT NOT NULL,
    "GrantdateE" TEXT NOT NULL,
    "GrantdateY" TEXT NOT NULL,
    "GrantdateM" TEXT NOT NULL,
    "GrantdateD" TEXT NOT NULL,
    "TranspotationBureauchiefName" TEXT NOT NULL,
    "EntryNoCarNo" TEXT NOT NULL,
    "ReggrantdateE" TEXT NOT NULL,
    "ReggrantdateY" TEXT NOT NULL,
    "ReggrantdateM" TEXT NOT NULL,
    "ReggrantdateD" TEXT NOT NULL,
    "FirstregistdateE" TEXT NOT NULL,
    "FirstregistdateY" TEXT NOT NULL,
    "FirstregistdateM" TEXT NOT NULL,
    "CarName" TEXT NOT NULL,
    "CarNameCode" TEXT NOT NULL,
    "CarNo" TEXT NOT NULL,
    "Model" TEXT NOT NULL,
    "EngineModel" TEXT NOT NULL,
    "OwnernameLowLevelChar" TEXT NOT NULL,
    "OwnernameHighLevelChar" TEXT NOT NULL,
    "OwnerAddressChar" TEXT NOT NULL,
    "OwnerAddressNumValue" TEXT NOT NULL,
    "OwnerAddressCode" TEXT NOT NULL,
    "UsernameLowLevelChar" TEXT NOT NULL,
    "UsernameHighLevelChar" TEXT NOT NULL,
    "UserAddressChar" TEXT NOT NULL,
    "UserAddressNumValue" TEXT NOT NULL,
    "UserAddressCode" TEXT NOT NULL,
    "UseheadqrterChar" TEXT NOT NULL,
    "UseheadqrterNumValue" TEXT NOT NULL,
    "UseheadqrterCode" TEXT NOT NULL,
    "CarKind" TEXT NOT NULL,
    "Use" TEXT NOT NULL,
    "PrivateBusiness" TEXT NOT NULL,
    "CarShape" TEXT NOT NULL,
    "CarShapeCode" TEXT NOT NULL,
    "NoteCap" TEXT NOT NULL,
    "Cap" TEXT NOT NULL,
    "NoteMaxloadage" TEXT NOT NULL,
    "Maxloadage" TEXT NOT NULL,
    "NoteCarWgt" TEXT NOT NULL,
    "CarWgt" TEXT NOT NULL,
    "NoteCarTotalWgt" TEXT NOT NULL,
    "CarTotalWgt" TEXT NOT NULL,
    "NoteLength" TEXT NOT NULL,
    "Length" TEXT NOT NULL,
    "NoteWidth" TEXT NOT NULL,
    "Width" TEXT NOT NULL,
    "NoteHeight" TEXT NOT NULL,
    "Height" TEXT NOT NULL,
    "FfAxWgt" TEXT NOT NULL,
    "FrAxWgt" TEXT NOT NULL,
    "RfAxWgt" TEXT NOT NULL,
    "RrAxWgt" TEXT NOT NULL,
    "Displacement" TEXT NOT NULL,
    "FuelClass" TEXT NOT NULL,
    "ModelSpecifyNo" TEXT NOT NULL,
    "ClassifyAroundNo" TEXT NOT NULL,
    "ValidPeriodExpirdateE" TEXT NOT NULL,
    "ValidPeriodExpirdateY" TEXT NOT NULL,
    "ValidPeriodExpirdateM" TEXT NOT NULL,
    "ValidPeriodExpirdateD" TEXT NOT NULL,
    "NoteInfo" TEXT NOT NULL,
    "TwodimensionCodeInfoEntryNoCarNo" TEXT NOT NULL,
    "TwodimensionCodeInfoCarNo" TEXT NOT NULL,
    "TwodimensionCodeInfoValidPeriodExpirdate" TEXT NOT NULL,
    "TwodimensionCodeInfoModel" TEXT NOT NULL,
    "TwodimensionCodeInfoModelSpecifyNoClassifyAroundNo" TEXT NOT NULL,
    "TwodimensionCodeInfoCharInfo" TEXT NOT NULL,
    "TwodimensionCodeInfoEngineModel" TEXT NOT NULL,
    "TwodimensionCodeInfoCarNoStampPlace" TEXT NOT NULL,
    "TwodimensionCodeInfoFirstregistdate" TEXT NOT NULL,
    "TwodimensionCodeInfoFfAxWgt" TEXT NOT NULL,
    "TwodimensionCodeInfoFrAxWgt" TEXT NOT NULL,
    "TwodimensionCodeInfoRfAxWgt" TEXT NOT NULL,
    "TwodimensionCodeInfoRrAxWgt" TEXT NOT NULL,
    "TwodimensionCodeInfoNoiseReg" TEXT NOT NULL,
    "TwodimensionCodeInfoNearNoiseReg" TEXT NOT NULL,
    "TwodimensionCodeInfoDriveMethod" TEXT NOT NULL,
    "TwodimensionCodeInfoOpacimeterMeasCar" TEXT NOT NULL,
    "TwodimensionCodeInfoNoxPmMeasMode" TEXT NOT NULL,
    "TwodimensionCodeInfoNoxValue" TEXT NOT NULL,
    "TwodimensionCodeInfoPmValue" TEXT NOT NULL,
    "TwodimensionCodeInfoSafeStdDate" TEXT NOT NULL,
    "TwodimensionCodeInfoFuelClassCode" TEXT NOT NULL,
    "RegistCarLightCar" TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT car_inspection_org_unique UNIQUE (tenant_id, "ElectCertMgNo", "GrantdateE", "GrantdateY", "GrantdateM", "GrantdateD")
);

CREATE INDEX IF NOT EXISTS idx_car_inspection_organization_id ON alc_api.car_inspection (tenant_id);
CREATE INDEX IF NOT EXISTS idx_car_inspection_car_id ON alc_api.car_inspection (tenant_id, "CarId");
CREATE INDEX IF NOT EXISTS idx_car_inspection_grantdate ON alc_api.car_inspection (tenant_id, "GrantdateY" DESC, "GrantdateM" DESC, "GrantdateD" DESC);
CREATE INDEX IF NOT EXISTS idx_car_inspection_valid_period ON alc_api.car_inspection (tenant_id, "TwodimensionCodeInfoValidPeriodExpirdate");

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.car_inspection ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.car_inspection FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'car_inspection' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.car_inspection
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.car_inspection TO alc_api_app;
GRANT USAGE, SELECT ON SEQUENCE alc_api.car_inspection_id_seq TO alc_api_app;

-- car_inspection_files テーブル
CREATE TABLE IF NOT EXISTS alc_api.car_inspection_files (
    uuid UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    type TEXT NOT NULL,
    "ElectCertMgNo" TEXT NOT NULL,
    "ElectCertPublishdateE" TEXT NOT NULL,
    "ElectCertPublishdateY" TEXT NOT NULL,
    "ElectCertPublishdateM" TEXT NOT NULL,
    "ElectCertPublishdateD" TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_car_inspection_files_organization_id ON alc_api.car_inspection_files (tenant_id);

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_files' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_files ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_files' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_files FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'car_inspection_files' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.car_inspection_files
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.car_inspection_files TO alc_api_app;

-- car_inspection_files_a テーブル
CREATE TABLE IF NOT EXISTS alc_api.car_inspection_files_a (
    uuid UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    type TEXT NOT NULL,
    "ElectCertMgNo" TEXT NOT NULL,
    "GrantdateE" TEXT NOT NULL,
    "GrantdateY" TEXT NOT NULL,
    "GrantdateM" TEXT NOT NULL,
    "GrantdateD" TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_car_inspection_files_a_organization_id ON alc_api.car_inspection_files_a (tenant_id);
CREATE INDEX IF NOT EXISTS idx_car_inspection_files_a_lookup ON alc_api.car_inspection_files_a (tenant_id, "ElectCertMgNo", "GrantdateE", "GrantdateY", "GrantdateM", "GrantdateD", type, deleted_at);

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_files_a' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_files_a ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_files_a' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_files_a FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'car_inspection_files_a' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.car_inspection_files_a
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.car_inspection_files_a TO alc_api_app;

-- car_inspection_files_b テーブル
CREATE TABLE IF NOT EXISTS alc_api.car_inspection_files_b (
    uuid UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    type TEXT NOT NULL,
    "ElectCertMgNo" TEXT NOT NULL,
    "GrantdateE" TEXT NOT NULL,
    "GrantdateY" TEXT NOT NULL,
    "GrantdateM" TEXT NOT NULL,
    "GrantdateD" TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_car_inspection_files_b_organization_id ON alc_api.car_inspection_files_b (tenant_id);
CREATE INDEX IF NOT EXISTS idx_car_inspection_files_b_lookup ON alc_api.car_inspection_files_b (tenant_id, "ElectCertMgNo", "GrantdateE", "GrantdateY", "GrantdateM", "GrantdateD", type, deleted_at);

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_files_b' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_files_b ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_files_b' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_files_b FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'car_inspection_files_b' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.car_inspection_files_b
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.car_inspection_files_b TO alc_api_app;

-- car_inspection_deregistration テーブル
CREATE TABLE IF NOT EXISTS alc_api.car_inspection_deregistration (
    id SERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    "CarId" TEXT NOT NULL,
    "TwodimensionCodeInfoCarNo" TEXT NOT NULL,
    "CarNo" TEXT NOT NULL,
    "ValidPeriodExpirdateE" TEXT NOT NULL,
    "ValidPeriodExpirdateY" TEXT NOT NULL,
    "ValidPeriodExpirdateM" TEXT NOT NULL,
    "ValidPeriodExpirdateD" TEXT NOT NULL,
    "TwodimensionCodeInfoValidPeriodExpirdate" TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT car_inspection_dereg_org_unique UNIQUE (tenant_id, "CarId", "TwodimensionCodeInfoCarNo", "ValidPeriodExpirdateE", "ValidPeriodExpirdateY", "ValidPeriodExpirdateM", "ValidPeriodExpirdateD")
);

CREATE INDEX IF NOT EXISTS idx_car_inspection_dereg_organization_id ON alc_api.car_inspection_deregistration (tenant_id);

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_deregistration' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_deregistration ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_deregistration' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_deregistration FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'car_inspection_deregistration' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.car_inspection_deregistration
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.car_inspection_deregistration TO alc_api_app;
GRANT USAGE, SELECT ON SEQUENCE alc_api.car_inspection_deregistration_id_seq TO alc_api_app;

-- car_inspection_deregistration_files テーブル
CREATE TABLE IF NOT EXISTS alc_api.car_inspection_deregistration_files (
    id SERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    "CarId" TEXT NOT NULL,
    "TwodimensionCodeInfoValidPeriodExpirdate" TEXT NOT NULL,
    file_uuid UUID NOT NULL REFERENCES alc_api.files(uuid),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT car_inspection_dereg_files_unique UNIQUE (tenant_id, "CarId", "TwodimensionCodeInfoValidPeriodExpirdate", file_uuid)
);

CREATE INDEX IF NOT EXISTS idx_car_inspection_dereg_files_organization_id ON alc_api.car_inspection_deregistration_files (tenant_id);

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_deregistration_files' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_deregistration_files ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_deregistration_files' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_deregistration_files FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'car_inspection_deregistration_files' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.car_inspection_deregistration_files
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.car_inspection_deregistration_files TO alc_api_app;
GRANT USAGE, SELECT ON SEQUENCE alc_api.car_inspection_deregistration_files_id_seq TO alc_api_app;

-- car_inspection_nfc_tags テーブル
CREATE TABLE IF NOT EXISTS alc_api.car_inspection_nfc_tags (
    id SERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    nfc_uuid TEXT NOT NULL,
    car_inspection_id INTEGER NOT NULL REFERENCES alc_api.car_inspection(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT car_inspection_nfc_tags_org_nfc_unique UNIQUE (tenant_id, nfc_uuid)
);

CREATE INDEX IF NOT EXISTS idx_nfc_tags_org ON alc_api.car_inspection_nfc_tags (tenant_id);
CREATE INDEX IF NOT EXISTS idx_nfc_tags_car_ins ON alc_api.car_inspection_nfc_tags (tenant_id, car_inspection_id);

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_nfc_tags' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_nfc_tags ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'car_inspection_nfc_tags' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.car_inspection_nfc_tags FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'car_inspection_nfc_tags' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.car_inspection_nfc_tags
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.car_inspection_nfc_tags TO alc_api_app;
GRANT USAGE, SELECT ON SEQUENCE alc_api.car_inspection_nfc_tags_id_seq TO alc_api_app;

-- pending_car_inspection_pdfs テーブル
CREATE TABLE IF NOT EXISTS alc_api.pending_car_inspection_pdfs (
    id SERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    file_uuid UUID NOT NULL,
    "ElectCertMgNo" TEXT NOT NULL,
    "GrantdateE" TEXT NOT NULL,
    "GrantdateY" TEXT NOT NULL,
    "GrantdateM" TEXT NOT NULL,
    "GrantdateD" TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT pending_pdf_unique UNIQUE (tenant_id, "ElectCertMgNo")
);

CREATE INDEX IF NOT EXISTS idx_pending_pdf_org_ecmn ON alc_api.pending_car_inspection_pdfs (tenant_id, "ElectCertMgNo");

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'pending_car_inspection_pdfs' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.pending_car_inspection_pdfs ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'pending_car_inspection_pdfs' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.pending_car_inspection_pdfs FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'pending_car_inspection_pdfs' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.pending_car_inspection_pdfs
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.pending_car_inspection_pdfs TO alc_api_app;
GRANT USAGE, SELECT ON SEQUENCE alc_api.pending_car_inspection_pdfs_id_seq TO alc_api_app;

-- sso_provider_configs テーブル
CREATE TABLE IF NOT EXISTS alc_api.sso_provider_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    provider TEXT NOT NULL,
    client_id TEXT NOT NULL,
    client_secret_encrypted TEXT NOT NULL,
    external_org_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    woff_id TEXT,
    CONSTRAINT sso_provider_configs_organization_id_provider_key UNIQUE (tenant_id, provider),
    CONSTRAINT sso_provider_configs_provider_client_id_key UNIQUE (provider, client_id),
    CONSTRAINT sso_provider_configs_provider_external_org_id_key UNIQUE (provider, external_org_id)
);

CREATE INDEX IF NOT EXISTS idx_sso_provider_configs_lookup ON alc_api.sso_provider_configs (provider, external_org_id) WHERE enabled = true;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'sso_provider_configs' AND c.relrowsecurity) THEN
        ALTER TABLE alc_api.sso_provider_configs ENABLE ROW LEVEL SECURITY;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'alc_api' AND c.relname = 'sso_provider_configs' AND c.relforcerowsecurity) THEN
        ALTER TABLE alc_api.sso_provider_configs FORCE ROW LEVEL SECURITY;
    END IF;
END $$;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'sso_provider_configs' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.sso_provider_configs
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT ALL ON alc_api.sso_provider_configs TO alc_api_app;
