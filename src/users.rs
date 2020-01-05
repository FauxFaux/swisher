use rand::AsByteSliceMut;
use std::convert::TryInto;

pub struct MasterKey {
    id: [u8; 3],
    key: [u8; 32],
}

pub type RoleId = [u8; 6];

impl MasterKey {
    pub fn new(from: &str) -> MasterKey {
        let key = mac(b"making a key", from.as_bytes());
        let id = mac(b"identifying a key", &key)[..3]
            .try_into()
            .expect("fixed slice");
        MasterKey { key, id }
    }

    //         S - swisher
    //         1 - version(
    // 3/4 bytes - master key id
    // 6/8 bytes - role id?
    // 6/8 bytes - entropy?
    pub fn access_key_for(&self, role_id: RoleId) -> String {
        let mut ret = String::with_capacity(22);
        ret.push_str("S1");
        ret.push_str(&pack(&self.id));
        ret.push_str(&pack(&role_id));
        ret.push_str(&pack(&rand::random::<u64>().to_le_bytes()[..6]));
        assert_eq!(22, ret.len());
        ret
    }

    pub fn parse_access(&self, key: &str) -> Result<RoleId, &'static str> {
        if 22 != key.len() {
            return Err("invalid length");
        }

        if !key.starts_with("S1") {
            return Err("invalid format / version");
        }

        let key = unpack(&key[2..]).ok_or("invalid encoding")?;

        if key[..3] != self.id {
            return Err("not issued by us");
        }

        Ok(key[3..9].try_into().expect("fixed slice"))
    }

    pub fn secret_key_for(&self, access_key: &str) -> String {
        // ..should this check it's valid?
        pack(&mac(&self.key, access_key.as_bytes()))
    }
}

fn pack(values: &[u8]) -> String {
    base64::encode_config(values, base64::URL_SAFE_NO_PAD)
}

fn unpack(value: &str) -> Option<Vec<u8>> {
    base64::decode_config(value, base64::URL_SAFE_NO_PAD).ok()
}

fn mac(key: &[u8], value: &[u8]) -> [u8; 32] {
    use hmac::Mac;
    let mut mac = hmac::Hmac::<sha2::Sha512Trunc256>::new_varkey(key).expect("valid key");
    mac.input(value);
    mac.result().code().try_into().expect("valid output size")
}

#[test]
fn key_derivation() {
    let master = MasterKey::new("");

    assert_eq!([187, 84, 139], master.id);
    assert_eq!([246, 204, 108], MasterKey::new("a").id);

    assert_eq!("u1SL", pack(&[187, 84, 139]));
    assert_eq!(
        [187, 84, 139],
        unpack("u1SL").expect("static test").as_slice()
    );

    assert_eq!(
        "92yexZYU1g4Oiu7izxKaK34Rg3ElYwVkaFsl08J50Co",
        master.secret_key_for("abc")
    );

    let access = master.access_key_for([1, 2, 3, 4, 5, 6]);
    assert_eq!(22, access.len());
    assert_eq!("S1u1SLAQIDBAUG", &access[..2 + 4 + 8]);
    assert_eq!(
        [1, 2, 3, 4, 5, 6],
        master.parse_access(&access).expect("test data")
    );

    let access = master.access_key_for([1, 2, 3, 4, 5, 7]);
    assert_eq!(22, access.len());
    assert_eq!("S1u1SLAQIDBAUH", &access[..2 + 4 + 8]);
    assert_eq!(
        [1, 2, 3, 4, 5, 7],
        master.parse_access(&access).expect("test data")
    );
}
