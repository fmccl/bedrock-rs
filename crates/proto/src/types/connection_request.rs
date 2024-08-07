use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::sync::Arc;

use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use bedrockrs_core::int::{LE, VAR};
use bedrockrs_proto_core::error::ProtoCodecError;
use bedrockrs_proto_core::ProtoCodec;
use jsonwebtoken::{DecodingKey, Validation};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ConnectionRequest {
    /// Array of Base64 encoded JSON Web Token certificates to authenticate the player.
    ///
    /// The last certificate in the chain will have a property 'extraData' that contains player identity information including the XBL XUID (if the player was signed into XBL at the time of the connection).
    pub certificate_chain: Vec<BTreeMap<String, Value>>,
    /// Base64 encoded JSON Web Token that contains other relevant client properties.
    ///
    /// Properties Include:
    /// - SelfSignedId
    /// - ServerAddress = (unresolved url if applicable)
    /// - ClientRandomId
    /// - SkinId
    /// - SkinData
    /// - SkinImageWidth
    /// - SkinImageHeight
    /// - CapeData
    /// - CapeImageWidth
    /// - CapeImageHeight
    /// - SkinResourcePatch
    /// - SkinGeometryData
    /// - SkinGeometryDataEngineVersion
    /// - SkinAnimationData
    /// - PlayFabId
    /// - AnimatedImageData = Array of:
    ///   - Type
    ///   - Image
    ///   - ImageWidth
    ///   - ImageHeight
    ///   - Frames
    ///   - AnimationExpression
    /// - ArmSize
    /// - SkinColor
    /// - PersonaPieces = Array of:
    ///   - PackId
    ///   - PieceId
    ///   - IsDefault
    ///   - PieceType
    ///   - ProuctId
    /// - PieceTintColors = Array of:
    ///   - PieceType
    ///   - Colors = Array of color hexstrings
    /// - IsEduMode (if edu mode)
    /// - TenantId (if edu mode)
    /// - ADRole (if edu mode)
    /// - IsEditorMode
    /// - GameVersion
    /// - DeviceModel
    /// - DeviceOS = (see enumeration: BuildPlatform)
    /// - DefaultInputMode = (see enumeration: InputMode)
    /// - CurrentInputMode = (see enumeration: InputMode)
    /// - UIProfile = (see enumeration: UIProfile)
    /// - GuiScale
    /// - LanguageCode
    /// - PlatformUserId
    /// - ThirdPartyName
    /// - ThirdPartyNameOnly
    /// - PlatformOnlineId
    /// - PlatformOfflineId
    /// - DeviceId
    /// - TrustedSkin
    /// - PremiumSkin
    /// - PersonaSkin
    /// - OverrideSkin
    /// - CapeOnClassicSkin
    /// - CapeId
    /// - CompatibleWithClientSideChunkGen
    pub raw_token: BTreeMap<String, Value>,
}

impl ProtoCodec for ConnectionRequest {
    fn proto_serialize(&self, stream: &mut Vec<u8>) -> Result<(), ProtoCodecError>
    where
        Self: Sized,
    {
        todo!()
    }

    // TODO: Add microsoft auth
    // TODO: Validate jwts (This is hard, Zuri nor Vincent could help me)
    fn proto_deserialize(stream: &mut Cursor<&[u8]>) -> Result<Self, ProtoCodecError>
    where
        Self: Sized,
    {
        let mut certificate_chain: Vec<BTreeMap<String, Value>> = vec![];

        // read the ConnectionRequests length
        // (certificate_chain len + raw_token len + 8)
        // 8 = i32 len + i32 len (length of certificate_chain's len and raw_token's len)
        // can be ignored, other lengths are provided
        VAR::<u32>::proto_deserialize(stream)?;

        // read length of certificate_chain vec
        let certificate_chain_len = LE::<i32>::proto_deserialize(stream)?.into_inner();

        let certificate_chain_len = certificate_chain_len
            .try_into()
            .map_err(|e| ProtoCodecError::FromIntError(e))?;

        let mut certificate_chain_buf = vec![0; certificate_chain_len];

        // read string data (certificate_chain)
        stream
            .read_exact(&mut certificate_chain_buf)
            .map_err(|e| ProtoCodecError::IOError(Arc::new(e)))?;

        // transform into string
        let certificate_chain_string =
            String::from_utf8(certificate_chain_buf).map_err(|e| ProtoCodecError::UTF8Error(e))?;

        // parse certificate chain string into json
        let certificate_chain_json = serde_json::from_str(certificate_chain_string.as_str())
            .map_err(|e| ProtoCodecError::JsonError(Arc::new(e)))?;

        let certificate_chain_json_jwts = match certificate_chain_json {
            Value::Object(mut v) => {
                match v.get_mut("chain") {
                    None => {
                        // the certificate chain should always be an object with just an array of
                        // JWTs called "chain"
                        return Err(ProtoCodecError::FormatMismatch(String::from(
                            "Missing element \"chain\" in JWT certificate_chain",
                        )));
                    }
                    Some(v) => {
                        match v.take() {
                            Value::Array(v) => v,
                            other => {
                                // the certificate chain should always be an object with just an
                                // array of JWTs called "chain"
                                return Err(ProtoCodecError::FormatMismatch(format!("Expected \"chain\" in JWT certificate_chain to be an Array, but got {other:?}")));
                            }
                        }
                    }
                }
            }
            other => {
                // the certificate chain should always be an object with just an array of
                // JWTs called "chain"
                return Err(ProtoCodecError::FormatMismatch(format!(
                    "Expected Object in base of JWT certificate_chain, got {other:?}"
                )));
            }
        };

        let mut key_data = vec![];

        for jwt_json in certificate_chain_json_jwts {
            let jwt_string = match jwt_json {
                Value::String(str) => str,
                other => {
                    // the certificate chain's should always be a jwt string
                    return Err(ProtoCodecError::FormatMismatch(format!("Expected chain array in certificate_chain to just contain Strings, but got {other:?}")));
                }
            };

            // Extract header
            let jwt_header = jsonwebtoken::decode_header(&jwt_string)
                .map_err(|e| ProtoCodecError::JwtError(e))?;

            let mut jwt_validation = Validation::new(jwt_header.alg);
            // TODO: This definitely is not right. Even Zuri-MC doesn't understand this.. I may understand it.. I do understand it, update I don't.
            // TODO: Someone else should find out how this works
            jwt_validation.insecure_disable_signature_validation();
            jwt_validation.set_required_spec_claims::<&str>(&[]);

            // Is first jwt, use self-signed header from x5u
            if key_data.is_empty() {
                let x5u = match jwt_header.x5u {
                    None => {
                        return Err(ProtoCodecError::FormatMismatch(String::from(
                            "Expected x5u in JWT header",
                        )));
                    }
                    Some(ref v) => v.as_bytes(),
                };

                key_data = BASE64_STANDARD
                    .decode(x5u)
                    .map_err(|e| ProtoCodecError::Base64DecodeError(e))?;
            }

            // Decode the jwt string into a jwt object
            let jwt = jsonwebtoken::decode::<BTreeMap<String, Value>>(
                &jwt_string,
                &DecodingKey::from_ec_der(&key_data),
                &jwt_validation,
            )
            .map_err(|e| ProtoCodecError::JwtError(e))?;

            key_data = match jwt.claims.get("identityPublicKey") {
                None => return Err(ProtoCodecError::FormatMismatch(String::from("Expected identityPublicKey field in JWT for validation"))),
                Some(v) => match v {
                    Value::String(str) => match BASE64_STANDARD.decode(str.as_bytes()) {
                        Ok(v) => v,
                        Err(e) => return Err(ProtoCodecError::Base64DecodeError(e)),
                    },
                    other => return Err(ProtoCodecError::FormatMismatch(format!("Expected identityPublicKey field in JWT to be of type String, got {other:?}"))),
                },
            };

            certificate_chain.push(jwt.claims);
        }

        // read length of certificate_chain vec
        let raw_token_len = LE::<i32>::read(stream)
            .map_err(|e| ProtoCodecError::IOError(Arc::new(e)))?
            .into_inner();

        let raw_token_len = raw_token_len
            .try_into()
            .map_err(|e| ProtoCodecError::FromIntError(e))?;

        let mut raw_token_buf = vec![0; raw_token_len];

        // read string data (certificate_chain)
        stream
            .read_exact(&mut raw_token_buf)
            .map_err(|e| ProtoCodecError::IOError(Arc::new(e)))?;

        // transform into string
        let raw_token_string =
            String::from_utf8(raw_token_buf).map_err(|e| ProtoCodecError::UTF8Error(e))?;

        // Extract header
        let raw_token_jwt_header = jsonwebtoken::decode_header(&raw_token_string)
            .map_err(|e| ProtoCodecError::JwtError(e))?;

        let mut jwt_validation = Validation::new(raw_token_jwt_header.alg);
        // TODO: This definitely is not right. Even Zuri-MC doesn't understand this.. I may understand it.. I do understand it, update I don't.
        // TODO: Someone else should find out how this works
        jwt_validation.insecure_disable_signature_validation();
        jwt_validation.set_required_spec_claims::<&str>(&[]);

        // Decode the jwt string into a jwt object
        let raw_token_jwt = jsonwebtoken::decode::<BTreeMap<String, Value>>(
            &raw_token_string,
            &DecodingKey::from_ec_der(&vec![]),
            &jwt_validation,
        )
        .map_err(|e| ProtoCodecError::JwtError(e))?;

        return Ok(Self {
            certificate_chain,
            raw_token: raw_token_jwt.claims,
        });
    }
}
