// secret.rs - シークレット設定（`type="password"`）をOSの資格情報保護へ預ける。
//
// 平文で `config.json` に置くと、設定ファイルを共有・バックアップしただけで秘密値が
// 漏れる。Windows は DPAPI で暗号化した文字列を、macOS はログインキーチェーンへ預けた
// 参照を保存し、`config.get()` が読み取り時に透過的に復号する（どちらも同一PCの同一
// OSユーザーだけが復号できる）。
//
// **Linux は同等のOS機能を使っていないため平文保存**（フォールバック許可: OS横断で
// 使える保護機構が無く、保存自体を諦めると設定機能が成立しないため）。
//
// 本アプリはクラウド推論を行わないので外部サービスのAPIキーを持たないが、配信ソフト
// 連携やローカルAPIの認証で秘密値が生まれうる。その場合は必ずこの経路へ乗せること。

/// 保護済みの値に付く接頭辞。この形のまま外部へ渡さないための目印でもある。
const PREFIX: &str = "enc:";

/// 設定キーの名前から「秘密値として扱うべきか」を判定する。
///
/// スキーマ（`config_schema()`）に載らない設定項目——設定画面が直接組み立てる
/// フィールドなど——は型申告だけでは拾えない。**キー名でも判定する**ことで、
/// 申告漏れによる平文保存を防ぐ。`secret::guard` のテストが両方向を検査する。
pub fn is_secret_key(key: &str) -> bool {
    const MARKERS: [&str; 6] = [
        "API_KEY",
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "WEBHOOK",
        "CREDENTIAL",
    ];
    MARKERS.iter().any(|marker| key.contains(marker))
}

/// 値が保護形式のままか（＝復号できていないか）。
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(PREFIX)
}

/// 秘密値を保存形式へ変換する。
///
/// 返す文字列を `config.json` へ書く。Windows は暗号文そのもの、macOS はキーチェーンの
/// 参照、Linux は平文。
pub fn encrypt(key: &str, value: &str) -> anyhow::Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    platform::encrypt(key, value)
}

/// 保存形式から秘密値へ戻す。保護形式でない値はそのまま返す。
///
/// 復号できない場合は**保護形式のまま返す**。呼び出し元（`config.get()`）が
/// 「この端末では使えない値」と判断して未設定として扱うため、ここで空へ落とさない
/// （空にすると、値が保存されているのに消えたように見えて原因が追えない）。
pub fn decrypt(value: &str) -> String {
    if !is_encrypted(value) {
        return value.to_string();
    }
    platform::decrypt(value).unwrap_or_else(|| value.to_string())
}

/// 保存済みの秘密値の実体を消す。
///
/// 設定を空にしたときに呼ぶ。macOS はキーチェーン側の項目が残るため、`config.json`
/// から参照を消すだけでは実体が残り続ける。
pub fn forget(key: &str) {
    platform::forget(key);
}

#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::c_void;
    use std::ptr;

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };

    const TAG: &str = "enc:dpapi:v1:";

    pub fn encrypt(_key: &str, value: &str) -> anyhow::Result<String> {
        let mut input = value.as_bytes().to_vec();
        let mut in_blob = CRYPT_INTEGER_BLOB {
            cbData: input.len() as u32,
            pbData: input.as_mut_ptr(),
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        // SAFETY: 入出力とも有効な blob を渡し、成功時だけ pbData を読んで LocalFree する。
        let ok = unsafe {
            CryptProtectData(
                &mut in_blob,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                0,
                &mut out_blob,
            )
        };
        if ok == 0 {
            anyhow::bail!("秘密値をこのPCの資格情報保護で暗号化できません");
        }
        // SAFETY: 成功したので pbData は cbData バイトの有効な領域。
        let bytes =
            unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }
                .to_vec();
        // SAFETY: CryptProtectData が確保した領域を返す（解放漏れを残さない）。
        unsafe { LocalFree(out_blob.pbData as *mut c_void) };
        Ok(format!("{TAG}{}", hex::encode(bytes)))
    }

    pub fn decrypt(value: &str) -> Option<String> {
        let hex_text = value.strip_prefix(TAG)?;
        let mut input = hex::decode(hex_text).ok()?;
        let mut in_blob = CRYPT_INTEGER_BLOB {
            cbData: input.len() as u32,
            pbData: input.as_mut_ptr(),
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        // SAFETY: encrypt と同じ約束。失敗時は pbData を触らない。
        let ok = unsafe {
            CryptUnprotectData(
                &mut in_blob,
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                0,
                &mut out_blob,
            )
        };
        if ok == 0 {
            // 別PC・別Windowsユーザーの config.json を持ち込んだ場合はここへ来る。
            return None;
        }
        // SAFETY: 成功したので pbData は cbData バイトの有効な領域。
        let bytes =
            unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }
                .to_vec();
        // SAFETY: CryptUnprotectData が確保した領域を返す。
        unsafe { LocalFree(out_blob.pbData as *mut c_void) };
        String::from_utf8(bytes).ok()
    }

    pub fn forget(_key: &str) {
        // DPAPI は暗号文そのものを config.json へ置くため、外部に実体が残らない。
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };

    const TAG: &str = "enc:keychain:v1:";
    const SERVICE: &str = "com.picovtuber.desktop";

    pub fn encrypt(key: &str, value: &str) -> anyhow::Result<String> {
        set_generic_password(SERVICE, key, value.as_bytes()).map_err(|error| {
            anyhow::anyhow!("秘密値をログインキーチェーンへ保存できません: {error}")
        })?;
        // config.json には参照だけを置く（秘密そのものは残らない）。
        Ok(format!("{TAG}{key}"))
    }

    pub fn decrypt(value: &str) -> Option<String> {
        let key = value.strip_prefix(TAG)?;
        let bytes = get_generic_password(SERVICE, key).ok()?;
        String::from_utf8(bytes).ok()
    }

    pub fn forget(key: &str) {
        // 参照を消すだけでは実体が残る。キーチェーン側も消す。
        delete_generic_password(SERVICE, key).ok();
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    // フォールバック許可: Linux にはOS横断で使える同等の保護機構が無いため平文で保存する。
    // 保存自体を諦めると設定機能が成立しない。この事実は SPEC.md と MANUAL.md に明記する。
    pub fn encrypt(_key: &str, value: &str) -> anyhow::Result<String> {
        Ok(value.to_string())
    }

    pub fn decrypt(_value: &str) -> Option<String> {
        // 平文保存なので保護形式の値は復号できない（別OSの config.json を持ち込んだ場合）。
        None
    }

    pub fn forget(_key: &str) {}
}

/// 秘密値の申告漏れを検出するガード。
#[cfg(test)]
mod guard {
    use crate::guard_scan;

    /// `config_schema()` で秘密っぽいキー名を申告しているのに `password` 型でない箇所を探す。
    ///
    /// 型申告が漏れると平文で `config.json` へ書かれる。
    #[test]
    fn 秘密っぽいキーはpassword型で申告する() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "secret.rs" {
                continue; // 判定の定義元
            }
            for (index, line) in source.body().lines().enumerate() {
                let Some(start) = line.find("PICOVTUBER_") else {
                    continue;
                };
                let key: String = line
                    .split_at(start)
                    .1
                    .chars()
                    .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                    .collect();
                if !super::is_secret_key(&key) {
                    continue;
                }
                // 同じ Field 宣言のなかに password 型の申告があるか（前後2行まで見る）。
                let window: Vec<&str> = source
                    .body()
                    .lines()
                    .skip(index.saturating_sub(2))
                    .take(5)
                    .collect();
                if !window.iter().any(|line| line.contains("password")) {
                    offenders.push(format!("{}:{} ({key})", source.path.display(), index + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "秘密値の設定キーは type=\"password\" で申告してください\
             （申告が漏れると config.json へ平文で保存されます）: {offenders:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{decrypt, encrypt, forget, is_encrypted, is_secret_key};

    #[test]
    fn キー名から秘密値を判定する() {
        assert!(is_secret_key("PICOVTUBER_STREAM_API_KEY"));
        assert!(is_secret_key("PICOVTUBER_OBS_PASSWORD"));
        assert!(is_secret_key("PICOVTUBER_RELAY_TOKEN"));
        assert!(is_secret_key("PICOVTUBER_SIGNING_SECRET"));
        assert!(is_secret_key("PICOVTUBER_NOTIFY_WEBHOOK"));
        assert!(is_secret_key("PICOVTUBER_STORE_CREDENTIAL"));
        // 秘密でないキーを巻き込まない（巻き込むと設定画面が伏せ字だらけになる）。
        assert!(!is_secret_key("PICOVTUBER_LIPSYNC_GAIN"));
        assert!(!is_secret_key("PICOVTUBER_MODELS_DIR"));
    }

    #[test]
    fn 保護形式の値を見分ける() {
        assert!(is_encrypted("enc:dpapi:v1:00ff"));
        assert!(is_encrypted("enc:keychain:v1:PICOVTUBER_X"));
        assert!(!is_encrypted("plain"));
        assert!(!is_encrypted(""));
    }

    #[test]
    fn 空の秘密値は保存形式を作らない() {
        assert_eq!(encrypt("PICOVTUBER_TEST_TOKEN", "").unwrap(), "");
    }

    #[test]
    fn 同じ端末で往復できる() {
        let stored = encrypt("PICOVTUBER_TEST_TOKEN", "秘密の値").unwrap();
        assert_eq!(decrypt(&stored), "秘密の値");
        forget("PICOVTUBER_TEST_TOKEN");
    }

    /// 復号できない値を空へ落とすと、保存されているのに消えたように見えて原因が追えない。
    /// 保護形式のまま返し、`config.get()` 側で未設定として扱わせる。
    #[test]
    fn 復号できない値は保護形式のまま返す() {
        let broken = "enc:dpapi:v1:zzzz";
        assert_eq!(decrypt(broken), broken);
        assert!(is_encrypted(&decrypt(broken)));
    }
}
