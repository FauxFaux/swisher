use std::collections::HashMap;

use chrono::DateTime;
use chrono::NaiveDate;
use chrono::NaiveDateTime;
use chrono::TimeZone;
use chrono::Utc;
use lazy_static::lazy_static;
use log::debug;
use regex::Regex;
use warheadhateus::AWSAuth;
use warheadhateus::HttpRequestMethod;
use warheadhateus::Region;

type HeaderMap = HashMap<String, String>;
type AccessKey = String;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Validation {
    Invalid,
    Unsupported,
    Anonymous(HeaderMap),
    Valid(AccessKey, HeaderMap),
}

pub fn validate<F>(
    url: &str,
    secret_key: F,
    now: DateTime<Utc>,
    mut headers: HashMap<String, String>,
    method: HttpRequestMethod,
) -> Validation
where
    F: FnOnce(&str) -> String,
{
    let authorization = match headers.get("authorization") {
        Some(authorization) => authorization,
        None => return Validation::Anonymous(headers),
    };

    let date = match headers.get("x-amz-date") {
        Some(date) => NaiveDateTime::parse_from_str(date, "%Y%m%dT%H%M%SZ"),
        None => {
            debug!("date header missing: {:?}", headers.keys());
            return Validation::Invalid;
        }
    };

    let date = match date {
        Ok(date) => date,
        Err(_) => {
            debug!("invalid date: {:?}: {:?}", headers.get("x-amz-date"), date);
            return Validation::Invalid;
        }
    };

    let v4 = "AWS4-HMAC-SHA256 ";
    if !authorization.starts_with(v4) {
        return Validation::Unsupported;
    }

    let parts = match split_auth(&authorization[v4.len()..]) {
        Some(v) => v,
        None => return Validation::Invalid,
    };

    if parts
        .valid_date
        .signed_duration_since(now.naive_utc().date())
        .num_days()
        .abs()
        > 2
    {
        return Validation::Invalid;
    }

    if parts.region != "us-east-1" || parts.service != "s3" {
        return Validation::Unsupported;
    }

    let mut war = AWSAuth::new(url).expect("valid url?");

    war.set_request_type(method);
    war.set_payload_hash(&warheadhateus::hashed_data(None).unwrap());
    war.set_date(DateTime::from_utc(date, Utc));

    war.set_access_key_id(&parts.access_key);
    war.set_secret_access_key(&secret_key(&parts.access_key));

    war.set_region(Region::UsEast1);

    let mut clean_headers = HashMap::with_capacity(parts.signed_headers.len());
    for header in parts.signed_headers {
        match headers.remove(&header) {
            Some(value) => {
                war.add_header(&header, &value);
                clean_headers.insert(header, value);
            }
            None => return Validation::Invalid,
        }
    }

    let war = war.signature().expect("generated signature");

    // TODO: constant time comparison
    if parts.signature != war {
        return Validation::Invalid;
    }

    Validation::Valid(parts.access_key, clean_headers)
}

lazy_static! {
    static ref HEADER_REGEX: Regex = Regex::new(
        "^Credential=([^/ ,=]+)/(\\d{8})/([^/ ,=]+)/([^/ ,=]+)/aws4_request, \
        SignedHeaders=([^/ ,=]+), \
        Signature=([a-f0-9]{64})$"
    )
    .expect("static regex");
}

fn split_auth(header: &str) -> Option<AuthHeaderFields> {
    // copying here is sad, but Captures -> returned struct directly complains,
    // not sure why and not sure I care
    let captures: regex::Captures<'_> = HEADER_REGEX.captures(header)?;
    Some(AuthHeaderFields {
        access_key: captures[1].to_string(),
        valid_date: NaiveDate::parse_from_str(&captures[2], "%Y%m%d").expect("regex checked date"),
        region: captures[3].to_string(),
        service: captures[4].to_string(),
        signed_headers: captures[5].split(';').map(|s| s.to_string()).collect(),
        signature: captures[6].to_string(),
    })
}

struct AuthHeaderFields {
    access_key: String,
    valid_date: NaiveDate,
    region: String,
    service: String,
    signed_headers: Vec<String>,
    signature: String,
}

#[test]
fn canned_request() {
    pretty_env_logger::init();

    assert_eq!(
        validate(
            "http://localhost:8202/foo-bar",
            |_| "456".to_string(),
            Utc.ymd(2020, 1, 4).and_hms(22, 23, 24),
            owned(maplit::hashmap! {
                    "authorization" => "AWS4-HMAC-SHA256 Credential=123/20200104/us-east-1/s3/aws4_request, \
                        SignedHeaders=host;x-amz-acl;x-amz-content-sha256;x-amz-date, \
                        Signature=18597c785bfe3fbb32b93202dcf4023c4333312cffe354dd54903b23da336707",
                    "accept-encoding" => "identity",
                    "content-length" => "0",
                    "host" => "localhost:8202",
                    "x-amz-acl" => "private",
                    "x-amz-content-sha256" => "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                    "x-amz-date" => "20200104T204036Z",
            }),
            HttpRequestMethod::PUT
        ),
        Validation::Valid(
            "123".to_string(),
            owned(maplit::hashmap! {
                "host" => "localhost:8202",
                "x-amz-acl" => "private",
                "x-amz-content-sha256" => "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "x-amz-date" => "20200104T204036Z",
            })
        )
    );
}

#[cfg(test)]
fn owned(map: HashMap<&str, &str>) -> HashMap<String, String> {
    map.into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
