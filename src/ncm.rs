use aes::cipher::block_padding::{Pkcs7, UnpadError};
use aes::cipher::{BlockDecryptMut, KeyInit};
use base64::{Engine as _, engine::general_purpose};
use byteorder::{LittleEndian, ReadBytesExt};
use ecb::Decryptor;
use id3::{Tag, TagLike, Version};
use metaflac::block::PictureType;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

// 定义常量
const CORE_KEY: &[u8] = b"\x68\x7a\x48\x52\x41\x6d\x73\x6f\x35\x6b\x49\x6e\x62\x61\x78\x57";
const META_KEY: &[u8] = b"\x23\x31\x34\x6c\x6a\x6b\x5f\x21\x5c\x5d\x26\x30\x55\x3c\x27\x28";
const NCM_MAGIC: &[u8] = b"CTENFDAM";
const BUFFER_SIZE: usize = 16384;

type EcbAes128Decrypt = Decryptor<aes::Aes128>;

/// NCM 处理中的错误
#[derive(Error, Debug)]
pub enum NcmError {
    #[error("文件 IO 错误: {0}")]
    FileIo(#[from] io::Error),
    #[error("无效的 NCM 格式: {0}")]
    Format(String),
    #[error("解密失败: {0}")]
    Decrypt(String),
    #[error("元数据处理失败: {0}")]
    Metadata(String),
    #[error("音频标签处理失败: {0}")]
    Tagging(String),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ID3 标签错误: {0}")]
    Id3(#[from] id3::Error),
    #[error("FLAC 标签错误: {0}")]
    Metaflac(#[from] metaflac::Error),
    #[error("Hex 解码错误: {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("Base64 解码错误: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("FromUtf8 错误: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),
    #[error("无效的填充: {0}")]
    InvalidPadding(String),
}

// 手动实现 From<UnpadError> 因为它没有实现 std::error::Error
impl From<UnpadError> for NcmError {
    fn from(err: UnpadError) -> Self {
        NcmError::InvalidPadding(format!("{:?}", err))
    }
}

/// 生成 RC4 密钥流 (根据 Python 源码的逻辑)
fn generate_rc4_keystream(key_data: &[u8]) -> Vec<u8> {
    let key_length = key_data.len();
    let mut s_box = (0u8..=255).collect::<Vec<u8>>();
    let mut j: u8 = 0;

    // KSA 初始化
    for i in 0..256 {
        j = j
            .wrapping_add(s_box[i])
            .wrapping_add(key_data[i % key_length]);
        s_box.swap(i, j as usize);
    }

    // 根据 Python 脚本的非标准方式生成流
    let mut stream_256 = Vec::with_capacity(256);
    for i in 0..256 {
        let i_u8 = i as u8;
        let si = s_box[i];
        let sj = s_box[i_u8.wrapping_add(si) as usize];
        let val = s_box[si.wrapping_add(sj) as usize];
        stream_256.push(val);
    }

    // 旋转并重复以创建最终的密钥流
    let mut final_stream = Vec::with_capacity(BUFFER_SIZE);
    let rotated_stream: Vec<u8> = stream_256[1..]
        .iter()
        .chain(&stream_256[..1])
        .cloned()
        .collect();
    for _ in 0..(BUFFER_SIZE / 256) {
        final_stream.extend_from_slice(&rotated_stream);
    }

    final_stream
}

/// 从 NCM 文件中读取元数据和封面
fn read_ncm_file(file: &mut File) -> Result<(Vec<u8>, Value, Option<Vec<u8>>), NcmError> {
    // 验证文件头
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;
    if magic != NCM_MAGIC {
        return Err(NcmError::Format("无效的 NCM 文件头".to_string()));
    }

    file.seek(SeekFrom::Current(2))?;

    // 解密核心密钥
    let key_len = file.read_u32::<LittleEndian>()? as usize;
    let mut key_data = vec![0u8; key_len];
    file.read_exact(&mut key_data)?;
    key_data.iter_mut().for_each(|byte| *byte ^= 0x64);

    let core_cipher = EcbAes128Decrypt::new(CORE_KEY.into());
    let decrypted_key = core_cipher.decrypt_padded_vec_mut::<Pkcs7>(&mut key_data)?;

    let key_stream = generate_rc4_keystream(&decrypted_key[17..]);

    // 解密元数据
    let meta_len = file.read_u32::<LittleEndian>()? as usize;
    let meta_data = if meta_len > 0 {
        let mut meta_encrypted = vec![0u8; meta_len];
        file.read_exact(&mut meta_encrypted)?;
        meta_encrypted.iter_mut().for_each(|byte| *byte ^= 0x63);

        let mut b64_decoded = general_purpose::STANDARD.decode(&meta_encrypted[22..])?;

        let meta_cipher = EcbAes128Decrypt::new(META_KEY.into());
        let decrypted_meta = meta_cipher.decrypt_padded_vec_mut::<Pkcs7>(&mut b64_decoded)?;

        let json_str = String::from_utf8(decrypted_meta.to_vec())?;
        serde_json::from_str(&json_str[6..])?
    } else {
        // 如果没有元数据，根据文件大小猜测格式
        let file_size = file.metadata()?.len();
        let format = if file_size > 1024 * 1024 * 16 {
            "flac"
        } else {
            "mp3"
        };
        serde_json::json!({ "format": format })
    };

    // 读取封面图片
    file.seek(SeekFrom::Current(5))?;
    let image_space = file.read_u32::<LittleEndian>()? as usize;
    let image_size = file.read_u32::<LittleEndian>()? as usize;
    let image_data = if image_size > 0 {
        let mut img_buf = vec![0u8; image_size];
        file.read_exact(&mut img_buf)?;
        Some(img_buf)
    } else {
        None
    };

    // **修正**: 跳过图片数据和实际音频数据之间的空白区域
    if image_space > image_size {
        file.seek(SeekFrom::Current((image_space - image_size) as i64))?;
    }

    Ok((key_stream, meta_data, image_data))
}

/// NCM 文件解密主函数
pub fn decrypt_and_dump(
    input_path: &Path,
    output_path: Option<&Path>,
    skip: bool,
) -> Result<PathBuf, NcmError> {
    let mut input_file = File::open(input_path)?;

    let (key_stream, meta_data, image_data) = read_ncm_file(&mut input_file)?;

    let format = meta_data["format"].as_str().unwrap_or("mp3").to_lowercase();

    let final_output_path = match output_path {
        Some(p) => p.with_extension(&format),
        None => input_path.with_extension(&format),
    };

    if skip && final_output_path.exists() {
        println!("文件已存在，跳过: {}", final_output_path.display());
        return Ok(final_output_path);
    }

    // 如果目录不存在，则创建
    if let Some(parent) = final_output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // 写入解密后的音频数据
    let mut output_file = File::create(&final_output_path)?;
    let mut buffer = [0u8; BUFFER_SIZE];
    loop {
        let bytes_read = input_file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        let data = &buffer[..bytes_read];
        let decrypted_data: Vec<u8> = data
            .iter()
            .zip(key_stream.iter().cycle())
            .map(|(d, k)| d ^ k)
            .collect();
        output_file.write_all(&decrypted_data)?;
    }

    // 写入元数据标签
    let title = meta_data["musicName"].as_str().unwrap_or("未知曲目");
    let album = meta_data["album"].as_str().unwrap_or("未知专辑");
    let artists: Vec<String> = meta_data["artist"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_array())
                .filter_map(|inner_arr| inner_arr[0].as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_else(|| vec!["未知艺术家".to_string()]);

    let track_no = meta_data["trackNo"].as_u64();

    if format == "mp3" {
        // **修正**: 尝试读取现有标签，如果不存在则创建新的。
        // 这样可以保留解密后的音频流中已有的标签。
        let mut tag = Tag::read_from_path(&final_output_path).unwrap_or_else(|_| Tag::new());

        tag.set_title(title);
        tag.set_album(album);
        tag.set_artist(artists.join("/"));
        if let Some(tn) = track_no {
            tag.set_track(tn as u32);
        }

        if let Some(img_data) = image_data {
            let mime_type = if img_data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                "image/png"
            } else {
                "image/jpeg"
            };
            let picture = id3::frame::Picture {
                mime_type: mime_type.to_string(),
                picture_type: id3::frame::PictureType::CoverFront,
                description: "Cover".to_string(),
                data: img_data,
            };
            // 移除旧封面，以防重复
            tag.remove_picture_by_type(id3::frame::PictureType::CoverFront);
            tag.add_frame(picture);
        }
        tag.write_to_path(&final_output_path, Version::Id3v23)?;
    } else if format == "flac" {
        let mut tag = metaflac::Tag::read_from_path(&final_output_path)?;
        let comments = tag.vorbis_comments_mut();
        comments.set_title(vec![title]);
        comments.set_album(vec![album]);
        comments.set_artist(artists);
        if let Some(tn) = track_no {
            comments.set("TRACKNUMBER", vec![tn.to_string()]);
        }

        if let Some(img_data) = image_data {
            let mime_type = if img_data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                "image/png"
            } else {
                "image/jpeg"
            };
            // 移除旧封面
            tag.remove_picture_type(PictureType::CoverFront);
            tag.add_picture(mime_type, PictureType::CoverFront, img_data);
        }
        tag.write_to_path(&final_output_path)?;
    }

    Ok(final_output_path)
}
