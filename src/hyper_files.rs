use std::convert::TryFrom;
use std::io;
use std::io::Write;

use failure::Error;
use hyper::body::Buf;
use hyper::body::HttpBody;
use hyper::body::Sender;
use md5::digest::FixedOutput;
use md5::digest::Input;
use tokio::io::AsyncReadExt as _;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt as _;
use tokio::prelude::AsyncRead;
use zstd::stream::raw::Operation;

use super::dir::ContentInfo;

pub async fn stream_pack<W: Unpin + AsyncWrite>(
    mut body: hyper::Body,
    mut out: W,
) -> Result<ContentInfo, Error> {
    let mut enc = zstd::stream::Encoder::new(io::Cursor::new(Vec::with_capacity(8 * 1024)), 3)?;
    enc.include_checksum(true)?;

    let mut length = 0;
    let mut md5 = md5::Md5::default();

    while let Some(data) = body.data().await {
        // typically 8 - 128kB chunks
        let mut data = data?;
        md5.input(&data);
        length += u64::try_from(data.len())?;

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

    let md5_base64 = base64::encode(&md5.fixed_result());

    Ok(ContentInfo { length, md5_base64 })
}

pub async fn stream_unpack<R: Unpin + AsyncRead>(
    mut from: R,
    mut sender: Sender,
) -> Result<(), Error> {
    let mut dec = zstd::stream::raw::Decoder::new()?;
    let mut inp = Vec::with_capacity(16 * 1024);

    loop {
        let found = {
            let mut buf = [0u8; 8 * 1024];
            let found = from.read(&mut buf).await?;
            inp.extend_from_slice(&buf[..found]);
            found
        };

        loop {
            let mut buf = [0u8; 16 * 1024];
            let status = dec.run_on_buffers(&inp, &mut buf)?;
            inp.drain(..status.bytes_read);
            if 0 == status.bytes_written {
                break;
            }

            sender
                .send_data(buf[..status.bytes_written].to_vec().into())
                .await?;
        }

        if 0 == found {
            if inp.is_empty() {
                // it doesn't want to write anything (previous loop condition),
                // we can't feed it any more data (found), and
                // it read everything that we had available
                return Ok(());
            }

            return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
        }
    }
}
