use std::io;
use std::io::Write;

use failure::Error;
use hyper::body::Buf;
use hyper::body::HttpBody;
use tokio::fs;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt as _;

use super::dir::Intermediate;

pub async fn write_temp_file<W: Unpin + AsyncWrite>(
    mut body: hyper::Body,
    mut out: W,
) -> Result<Intermediate, Error> {
    let mut enc = zstd::stream::Encoder::new(io::Cursor::new(Vec::with_capacity(8 * 1024)), 3)?;
    enc.include_checksum(true)?;

    while let Some(data) = body.data().await {
        // typically 8 - 128kB chunks
        let mut data = data?;
        while !data.is_empty() {
            let written = enc.write(&data)?;
            data.advance(written);
            let cursor = enc.get_mut();
            let vec = cursor.get_mut();

            // frequently (for compressible data), the write has not caused any new frames
            if !vec.is_empty() {
                out.write_all(vec).await?;
                vec.clear();
                cursor.set_position(0);
            }
        }
    }

    out.write_all(enc.finish()?.get_ref()).await?;

    Ok(Intermediate {
        content_length: 0,
        content_md5_base64: "".to_string(),
    })
}
