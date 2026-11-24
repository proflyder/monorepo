use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;
use x509_parser::asn1_rs::{Any, FromDer, Tag};
use x509_parser::certificate::X509Certificate;
use x509_parser::oid_registry::Oid;
use x509_parser::parse_x509_certificate;
use x509_parser::pem::parse_x509_pem;
use x509_parser::time::ASN1Time;

const MIN_CHAIN_CERTIFICATES: usize = 2;
const MAX_CHAIN_CERTIFICATES: usize = 8;
const SECURITY_LEVEL_TEE: u32 = 1;
const SECURITY_LEVEL_STRONGBOX: u32 = 2;
const ROOT_OF_TRUST_TAG: u32 = 704;

const GOOGLE_RSA_ROOT: &str = "-----BEGIN CERTIFICATE-----
MIIFHDCCAwSgAwIBAgIJAPHBcqaZ6vUdMA0GCSqGSIb3DQEBCwUAMBsxGTAXBgNV
BAUTEGY5MjAwOWU4NTNiNmIwNDUwHhcNMjIwMzIwMTgwNzQ4WhcNNDIwMzE1MTgw
NzQ4WjAbMRkwFwYDVQQFExBmOTIwMDllODUzYjZiMDQ1MIICIjANBgkqhkiG9w0B
AQEFAAOCAg8AMIICCgKCAgEAr7bHgiuxpwHsK7Qui8xUFmOr75gvMsd/dTEDDJdS
Sxtf6An7xyqpRR90PL2abxM1dEqlXnf2tqw1Ne4Xwl5jlRfdnJLmN0pTy/4lj4/7
tv0Sk3iiKkypnEUtR6WfMgH0QZfKHM1+di+y9TFRtv6y//0rb+T+W8a9nsNL/ggj
nar86461qO0rOs2cXjp3kOG1FEJ5MVmFmBGtnrKpa73XpXyTqRxB/M0n1n/W9nGq
C4FSYa04T6N5RIZGBN2z2MT5IKGbFlbC8UrW0DxW7AYImQQcHtGl/m00QLVWutHQ
oVJYnFPlXTcHYvASLu+RhhsbDmxMgJJ0mcDpvsC4PjvB+TxywElgS70vE0XmLD+O
JtvsBslHZvPBKCOdT0MS+tgSOIfga+z1Z1g7+DVagf7quvmag8jfPioyKvxnK/Eg
sTUVi2ghzq8wm27ud/mIM7AY2qEORR8Go3TVB4HzWQgpZrt3i5MIlCaY504LzSRi
igHCzAPlHws+W0rB5N+er5/2pJKnfBSDiCiFAVtCLOZ7gLiMm0jhO2B6tUXHI/+M
RPjy02i59lINMRRev56GKtcd9qO/0kUJWdZTdA2XoS82ixPvZtXQpUpuL12ab+9E
aDK8Z4RHJYYfCT3Q5vNAXaiWQ+8PTWm2QgBR/bkwSWc+NpUFgNPN9PvQi8WEg5Um
AGMCAwEAAaNjMGEwHQYDVR0OBBYEFDZh4QB8iAUJUYtEbEf/GkzJ6k8SMB8GA1Ud
IwQYMBaAFDZh4QB8iAUJUYtEbEf/GkzJ6k8SMA8GA1UdEwEB/wQFMAMBAf8wDgYD
VR0PAQH/BAQDAgIEMA0GCSqGSIb3DQEBCwUAA4ICAQB8cMqTllHc8U+qCrOlg3H7
174lmaCsbo/bJ0C17JEgMLb4kvrqsXZs01U3mB/qABg/1t5Pd5AORHARs1hhqGIC
W/nKMav574f9rZN4PC2ZlufGXb7sIdJpGiO9ctRhiLuYuly10JccUZGEHpHSYM2G
tkgYbZba6lsCPYAAP83cyDV+1aOkTf1RCp/lM0PKvmxYN10RYsK631jrleGdcdkx
oSK//mSQbgcWnmAEZrzHoF1/0gso1HZgIn0YLzVhLSA/iXCX4QT2h3J5z3znluKG
1nv8NQdxei2DIIhASWfu804CA96cQKTTlaae2fweqXjdN1/v2nqOhngNyz1361mF
mr4XmaKH/ItTwOe72NI9ZcwS1lVaCvsIkTDCEXdm9rCNPAY10iTunIHFXRh+7KPz
lHGewCq/8TOohBRn0/NNfh7uRslOSZ/xKbN9tMBtw37Z8d2vvnXq/YWdsm1+JLVw
n6yYD/yacNJBlwpddla8eaVMjsF6nBnIgQOf9zKSe06nSTqvgwUHosgOECZJZ1Eu
zbH4yswbt02tKtKEFhx+v+OTge/06V+jGsqTWLsfrOCNLuA8H++z+pUENmpqnnHo
vaI47gC+TNpkgYGkkBT6B/m/U01BuOBBTzhIlMEZq9qkDWuM2cA5kW5V3FJUcfHn
w1IdYIg2Wxg7yHcQZemFQg==
-----END CERTIFICATE-----";

const GOOGLE_EC_ROOT: &str = "-----BEGIN CERTIFICATE-----
MIICIjCCAaigAwIBAgIRAISp0Cl7DrWK5/8OgN52BgUwCgYIKoZIzj0EAwMwUjEc
MBoGA1UEAwwTS2V5IEF0dGVzdGF0aW9uIENBMTEQMA4GA1UECwwHQW5kcm9pZDET
MBEGA1UECgwKR29vZ2xlIExMQzELMAkGA1UEBhMCVVMwHhcNMjUwNzE3MjIzMjE4
WhcNMzUwNzE1MjIzMjE4WjBSMRwwGgYDVQQDDBNLZXkgQXR0ZXN0YXRpb24gQ0Ex
MRAwDgYDVQQLDAdBbmRyb2lkMRMwEQYDVQQKDApHb29nbGUgTExDMQswCQYDVQQG
EwJVUzB2MBAGByqGSM49AgEGBSuBBAAiA2IABCPaI3FO3z5bBQo8cuiEas4HjqCt
G/mLFfRT0MsIssPBEEU5Cfbt6sH5yOAxqEi5QagpU1yX4HwnGb7OtBYpDTB57uH5
Eczm34A5FNijV3s0/f0UPl7zbJcTx6xwqMIRq6NCMEAwDwYDVR0TAQH/BAUwAwEB
/zAOBgNVHQ8BAf8EBAMCAQYwHQYDVR0OBBYEFFIyuyz7RkOb3NaBqQ5lZuA0QepA
MAoGCCqGSM49BAMDA2gAMGUCMETfjPO/HwqReR2CS7p0ZWoD/LHs6hDi422opifH
EUaYLxwGlT9SLdjkVpz0UUOR5wIxAIoGyxGKRHVTpqpGRFiJtQEOOTp/+s1GcxeY
uR2zh/80lQyu9vAFCj6E4AXc+osmRg==
-----END CERTIFICATE-----";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    #[error("attestation chain has {0} certificates, outside 2..=8")]
    Length(usize),
    #[error("attestation certificate {0} does not parse")]
    Parse(usize),
    #[error("attestation certificate {index} ({subject}) is not valid now")]
    Expired { index: usize, subject: String },
    #[error("attestation certificate {0} is not signed by certificate {1}")]
    Link(usize, usize),
    #[error("attestation certificate {0} is not a CA")]
    Authority(usize),
    #[error("attestation verification time is out of range")]
    Clock,
    #[error("attestation chain does not reach a pinned root")]
    Anchor,
    #[error("pinned trust anchors are malformed")]
    Roots,
    #[error("attestation extension is malformed")]
    Extension,
    #[error("attestation challenge differs")]
    Challenge,
    #[error("security level is not TEE or StrongBox")]
    SecurityLevel,
    #[error("attested key differs from the enrolled key")]
    PublicKey,
    #[error("attestation certificate encoding is invalid")]
    Encoding,
    #[error("verified boot is not verified and locked")]
    VerifiedBoot,
}

pub struct Roots {
    der: Vec<Vec<u8>>,
}

impl Roots {
    pub fn fingerprints(&self) -> Vec<String> {
        self.der
            .iter()
            .map(|der| hex::encode(Sha256::digest(der)))
            .collect()
    }

    pub fn pinned() -> Self {
        Self::parse_pems(&[GOOGLE_RSA_ROOT, GOOGLE_EC_ROOT]).expect("pinned roots parse")
    }

    pub fn parse_pems(pems: &[&str]) -> Result<Self, Error> {
        let mut der = Vec::new();
        for pem in pems {
            let (_, parsed) = parse_x509_pem(pem.as_bytes()).map_err(|_| Error::Roots)?;
            der.push(parsed.contents);
        }
        if der.is_empty() {
            return Err(Error::Roots);
        }
        Ok(Self { der })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    pub security_level: u32,
    pub verified_boot: String,
    pub device_locked: bool,
    pub fingerprint: String,
}

struct KeyDescription {
    security_level: u32,
    challenge: Vec<u8>,
    verified_boot: String,
    device_locked: bool,
}

pub fn verify(
    chain: &[String],
    challenge: &str,
    public_key_der: &[u8],
    roots: &Roots,
    now_unix: u64,
) -> Result<Attestation, Error> {
    let result = verify_chain(chain, challenge, public_key_der, roots, now_unix);
    if let Err(error) = &result {
        log_rejection(chain, error);
    }
    result
}

fn verify_chain(
    chain: &[String],
    challenge: &str,
    public_key_der: &[u8],
    roots: &Roots,
    now_unix: u64,
) -> Result<Attestation, Error> {
    if !(MIN_CHAIN_CERTIFICATES..=MAX_CHAIN_CERTIFICATES).contains(&chain.len()) {
        return Err(Error::Length(chain.len()));
    }
    let ders = chain
        .iter()
        .map(|item| decode_certificate(item))
        .collect::<Result<Vec<_>, _>>()?;
    let certificates = ders
        .iter()
        .enumerate()
        .map(|(index, der)| {
            parse_x509_certificate(der)
                .map(|(_, certificate)| certificate)
                .map_err(|_| Error::Parse(index))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let now = ASN1Time::from_timestamp(i64::try_from(now_unix).unwrap_or(i64::MAX))
        .map_err(|_| Error::Clock)?;
    let anchored = chain_anchor(&certificates, &ders, roots);
    let anchor_index = certificates.len() - 1;
    for (index, certificate) in certificates.iter().enumerate() {
        if anchored && index == anchor_index {
            continue;
        }
        if !certificate.validity().is_valid_at(now) {
            return Err(Error::Expired {
                index,
                subject: certificate.subject().to_string(),
            });
        }
    }
    for (index, pair) in certificates.windows(2).enumerate() {
        pair[0]
            .verify_signature(Some(pair[1].public_key()))
            .map_err(|_| Error::Link(index, index + 1))?;
    }
    for (offset, certificate) in certificates[1..].iter().enumerate() {
        if !certificate
            .basic_constraints()
            .ok()
            .flatten()
            .is_some_and(|constraints| constraints.value.ca)
        {
            return Err(Error::Authority(offset + 1));
        }
    }
    if !anchored {
        return Err(Error::Anchor);
    }
    let leaf = &certificates[0];
    if leaf.public_key().raw != public_key_der {
        return Err(Error::PublicKey);
    }
    let oid = attestation_oid();
    let extension = leaf
        .extensions()
        .iter()
        .find(|extension| extension.oid == oid)
        .ok_or(Error::Extension)?;
    let description = key_description(extension.value)?;
    if description
        .challenge
        .as_slice()
        .ct_eq(challenge.as_bytes())
        .unwrap_u8()
        == 0
    {
        return Err(Error::Challenge);
    }
    if !matches!(
        description.security_level,
        SECURITY_LEVEL_TEE | SECURITY_LEVEL_STRONGBOX
    ) {
        return Err(Error::SecurityLevel);
    }
    if !description.device_locked || description.verified_boot != "verified" {
        tracing::warn!(
            security_level = description.security_level,
            verified_boot = %description.verified_boot,
            device_locked = description.device_locked,
            "key attestation rejected an unverified boot state"
        );
        return Err(Error::VerifiedBoot);
    }
    let mut hasher = Sha256::new();
    for der in &ders {
        hasher.update(u64::try_from(der.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(der);
    }
    hasher.update(description.challenge);
    hasher.update(description.security_level.to_be_bytes());
    hasher.update([u8::from(description.device_locked)]);
    Ok(Attestation {
        security_level: description.security_level,
        verified_boot: description.verified_boot,
        device_locked: description.device_locked,
        fingerprint: hex::encode(hasher.finalize()),
    })
}

fn decode_certificate(item: &str) -> Result<Vec<u8>, Error> {
    let normalized = item
        .trim()
        .chars()
        .map(|character| match character {
            '-' => '+',
            '_' => '/',
            other => other,
        })
        .collect::<String>();
    STANDARD_NO_PAD
        .decode(normalized.trim_end_matches('='))
        .map_err(|_| Error::Encoding)
}

fn log_rejection(chain: &[String], error: &Error) {
    let certificates = chain
        .iter()
        .enumerate()
        .map(|(index, item)| describe_certificate(index, item))
        .collect::<Vec<_>>();
    tracing::warn!(
        chain_length = chain.len(),
        certificates = %certificates.join("; "),
        error = %error,
        "key attestation chain rejected"
    );
}

fn describe_certificate(index: usize, item: &str) -> String {
    let Ok(der) = decode_certificate(item) else {
        return format!("{index}: undecodable base64");
    };
    let Ok((_, certificate)) = parse_x509_certificate(&der) else {
        return format!("{index}: does not parse");
    };
    format!(
        "{index}: subject={} issuer={} not_after={}",
        certificate.subject(),
        certificate.issuer(),
        certificate.validity().not_after
    )
}

fn chain_anchor(certificates: &[X509Certificate<'_>], ders: &[Vec<u8>], roots: &Roots) -> bool {
    let top = certificates.last().expect("chain is not empty");
    let top_der = ders.last().expect("chain is not empty");
    if roots
        .der
        .iter()
        .any(|root| root.as_slice() == top_der.as_slice())
    {
        return true;
    }
    roots.der.iter().any(|root| {
        let Ok((_, certificate)) = parse_x509_certificate(root) else {
            return false;
        };
        top.verify_signature(Some(certificate.public_key())).is_ok()
    })
}

fn children<'a>(sequence: &'a x509_parser::asn1_rs::Sequence<'a>) -> Result<Vec<Any<'a>>, Error> {
    let mut rest: &'a [u8] = sequence.content.as_ref();
    let mut items = Vec::new();
    while !rest.is_empty() {
        let (next, item) = Any::from_der(rest).map_err(|_| Error::Extension)?;
        items.push(item);
        rest = next;
    }
    Ok(items)
}

fn attestation_oid() -> Oid<'static> {
    Oid::from(&[1, 3, 6, 1, 4, 1, 11129, 2, 1, 17]).expect("attestation OID")
}

fn key_description(value: &[u8]) -> Result<KeyDescription, Error> {
    let (_, root) = Any::from_der(value).map_err(|_| Error::Extension)?;
    let sequence = root.as_sequence().map_err(|_| Error::Extension)?;
    let items = children(&sequence)?;
    let mut items = items.into_iter();
    let _version = items
        .next()
        .ok_or(Error::Extension)?
        .as_u32()
        .map_err(|_| Error::Extension)?;
    let security_level = items
        .next()
        .ok_or(Error::Extension)?
        .as_enumerated()
        .map_err(|_| Error::Extension)?
        .0;
    let _keymaster_version = items
        .next()
        .ok_or(Error::Extension)?
        .as_u32()
        .map_err(|_| Error::Extension)?;
    let _keymaster_level = items
        .next()
        .ok_or(Error::Extension)?
        .as_enumerated()
        .map_err(|_| Error::Extension)?
        .0;
    let challenge = items
        .next()
        .ok_or(Error::Extension)?
        .as_octetstring()
        .map_err(|_| Error::Extension)?
        .as_ref()
        .to_vec();
    let _unique_id = items.next().ok_or(Error::Extension)?;
    let _software = items.next().ok_or(Error::Extension)?;
    let tee_item = items.next().ok_or(Error::Extension)?;
    let tee = tee_item.as_sequence().map_err(|_| Error::Extension)?;
    let mut verified_boot = "unknown".to_owned();
    let mut device_locked = false;
    for item in children(&tee)? {
        if item.tag() != Tag(ROOT_OF_TRUST_TAG) {
            continue;
        }
        let (_, inner) = Any::from_der(item.as_bytes()).map_err(|_| Error::Extension)?;
        let trust = inner.as_sequence().map_err(|_| Error::Extension)?;
        let parts = children(&trust)?;
        let mut parts = parts.into_iter();
        let _boot_key = parts.next().ok_or(Error::Extension)?;
        device_locked = parts
            .next()
            .ok_or(Error::Extension)?
            .as_bool()
            .map_err(|_| Error::Extension)?;
        let state = parts
            .next()
            .ok_or(Error::Extension)?
            .as_enumerated()
            .map_err(|_| Error::Extension)?
            .0;
        verified_boot = match state {
            0 => String::from("verified"),
            1 => String::from("self_signed"),
            2 => String::from("unverified"),
            3 => String::from("failed"),
            _ => String::from("unknown"),
        };
    }
    Ok(KeyDescription {
        security_level,
        challenge,
        verified_boot,
        device_locked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_google_roots_have_the_expected_fingerprints() {
        let roots = Roots::pinned();
        assert_eq!(
            roots.fingerprints(),
            vec![
                "cedb1cb6dc896ae5ec797348bce9286753c2b38ee71ce0fbe34a9a1248800dfc",
                "6d9db4ce6c5c0b293166d08986e05774a8776ceb525d9e4329520de12ba4bcc0",
            ]
        );
    }

    #[test]
    fn an_empty_root_set_is_rejected() {
        assert!(Roots::parse_pems(&[]).is_err());
        assert!(Roots::parse_pems(&["not a certificate"]).is_err());
    }
}
