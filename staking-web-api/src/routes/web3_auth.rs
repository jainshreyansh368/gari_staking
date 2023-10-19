use crate::dto::ResponseData;
use crate::pool::StakingConfig;
use hmac::{Hmac, Mac};
use jwt::token::verified::VerifyWithKey;
use jwt::SignWithKey;
use rocket::{serde::json::Json, State};
use sha2::Sha256;
use std::{collections::BTreeMap, str::FromStr};
use tracing::{info, warn};

#[get(
    "/auth/login?<user_spl_token_owner>&<message>&<signature>",
    format = "application/json"
)]
pub async fn login(
    staking_config: &State<StakingConfig>,
    user_spl_token_owner: String,
    message: String,
    signature: String,
) -> Json<ResponseData<String>> {
    match verify_user_ownes_pubkey(&user_spl_token_owner, &message, &signature).await {
        Ok(is_verified) => {
            if !is_verified {
                return Json(ResponseData::new(
                    400,
                    "Signature verification failed".to_owned(),
                    None,
                ));
            }
            match generate_jwt_token(
                &user_spl_token_owner,
                &message,
                &signature,
                &staking_config.jwt_key,
            )
            .await
            {
                Ok(jwt_token) => Json(ResponseData::new(200, "".to_owned(), Some(jwt_token))),
                Err(error) => Json(ResponseData::new(500, error, None)),
            }
        }
        Err(error) => Json(ResponseData::new(400, error, None)),
    }
}

#[get(
    "/auth/verify?<user_spl_token_owner>&<jwt_token>",
    format = "application/json"
)]
pub async fn verify(
    staking_config: &State<StakingConfig>,
    user_spl_token_owner: String,
    jwt_token: String,
) -> Json<ResponseData<String>> {
    let status =
        if verify_jwt_token(&staking_config.jwt_key, &user_spl_token_owner, &jwt_token).await {
            "successful".to_owned()
        } else {
            "failed".to_owned()
        };
    Json(ResponseData::new(200, "".to_owned(), Some(status)))
}

async fn verify_user_ownes_pubkey(
    user_spl_token_owner: &str,
    message: &str,
    signature: &str,
) -> Result<bool, String> {
    let token_decoded = match bs58::decode(&user_spl_token_owner).into_vec() {
        Ok(pkey) => pkey,
        Err(error) => {
            warn!("user_spl_token_owner decode error: {}", error);
            return Err("Key decodeing failed".to_string());
        }
    };
    let pubkey = match ed25519_dalek::PublicKey::from_bytes(&token_decoded) {
        Ok(pubkey) => pubkey,
        Err(error) => {
            warn!("Invalid pubkey: {}", error);
            return Err("Not valid user wallet key".to_string());
        }
    };

    let signature = match bs58::decode(signature).into_vec() {
        Ok(signature_decode) => signature_decode,
        Err(error) => {
            warn!("signature decode error: {}", error);
            return Err("Signature decoding failed".to_string());
        }
    };

    let signature = match ed25519_dalek::Signature::from_bytes(&signature) {
        Ok(signature) => signature,
        Err(error) => {
            warn!("Bad signature: {}", error);
            return Err("Not valid signature".to_string());
        }
    };

    Ok(pubkey.verify_strict(message.as_bytes(), &signature).is_ok())
}

async fn generate_jwt_token(
    user_spl_token_owner: &str,
    message: &str,
    signature: &str,
    jwt_key: &str,
) -> Result<String, String> {
    let key: Hmac<Sha256> = match Hmac::new_from_slice(jwt_key.as_bytes()) {
        Ok(key) => key,
        Err(error) => {
            warn!("Invalid key: {}", error);
            return Err("Invalid key".to_string());
        }
    };
    let mut claims: BTreeMap<&str, &str> = BTreeMap::new();
    claims.insert("pubkey", user_spl_token_owner);
    claims.insert("message", message);
    claims.insert("signature", signature);

    let expiry = chrono::Local::now()
        .checked_add_days(chrono::Days::new(1))
        .unwrap()
        .to_string();

    claims.insert("expiry", &expiry);

    match claims.sign_with_key(&key) {
        Ok(jwt_token) => Ok(jwt_token),
        Err(error) => Err(error.to_string()),
    }
}

async fn verify_jwt_token(jwt_key: &str, user_spl_token_owner: &str, jwt_token: &str) -> bool {
    let key: Hmac<Sha256> = match Hmac::new_from_slice(jwt_key.as_bytes()) {
        Ok(key) => key,
        Err(error) => {
            warn!("Faulty JWT key: {}", error);
            return false;
        }
    };
    let claims: BTreeMap<String, String> = match jwt_token.verify_with_key(&key) {
        Ok(claims) => claims,
        Err(error) => {
            info!("JWT verification error: {}", error);
            warn!("Invalid JWT token passed!");
            return false;
        }
    };
    assert_eq!(claims["pubkey"], user_spl_token_owner);
    if !claims["pubkey"].eq(user_spl_token_owner) {
        info!("Wrong pubkey in JWT token");
        return false;
    }

    let expiry: chrono::DateTime<chrono::Local> =
        match chrono::DateTime::from_str(&claims["expiry"]) {
            Ok(expiry) => expiry,
            Err(error) => {
                warn!("Bad expiry string: {}", error);
                return false;
            }
        };
    let now = chrono::Local::now();
    if now.le(&expiry) {
        true
    } else {
        info!("JWT token is expired");
        false
    }
}
