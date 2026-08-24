use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions, Permissions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use lofty::{
    config::WriteOptions,
    ogg::tag::VorbisComments,
    picture::{Picture, PictureInformation},
    tag::TagExt,
};

use crate::error::TagError;

// 标准flac文件开头字节标识
const FLAC_MARKER: &[u8; 4] = b"fLaC";
// 音频基本信息：采样率，声道，MD5等等
const STREAM_INFO_BLOCK: u8 = 0;
// 预留空白空间
const PADDING_BLOCK: u8 = 1;
// 标题，歌手，专辑，歌词等文字标签
const VORBIS_COMMENT_BLOCK: u8 = 4;
// 封面图片
const PICTURE_BLOCK: u8 = 6;
const INVALID_BLOCK: u8 = 127;
const STREAM_INFO_BYTES: usize = 34;
// FLAC 元数据块头只有 24 位长度字段。
const MAX_BLOCK_BYTES: usize = 0x00FF_FFFF;
const DEFAULT_PADDING_BYTES: u64 = WriteOptions::DEFAULT_PREFERRED_PADDING as u64;
// 这是重写音频帧时唯一的大块工作缓冲区，不随歌曲大小增长。
const COPY_BUFFER_BYTES: usize = 256 * 1024;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

// 用同目录临时文件流式重写 FLAC 元数据，成功后替换原文件。
pub(crate) fn rewrite_flac_tags(
    path: &Path,
    comments: &VorbisComments,
    pictures: &[(Picture, PictureInformation)],
) -> Result<(), TagError> {
    // 让lofty将标签信息编码成二进制格式
    let mut encoded_comments = Vec::new();
    comments.dump_to(&mut encoded_comments, WriteOptions::default())?;
    if encoded_comments.len() > MAX_BLOCK_BYTES {
        return Err(TagError::FlacMetadataTooLarge);
    }

    // 打开原文件并保存权限
    let source = File::open(path)?;
    let permissions = source.metadata()?.permissions();
    // 创建临时文件
    let (temp_path, temp) = create_temp_file(path)?;

    // 执行重写
    let result = rewrite_to_temp(
        source,
        temp,
        &temp_path,
        permissions,
        path,
        &encoded_comments,
        pictures,
    );
    // 出错删除临时文件，原文件保持不变
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn rewrite_to_temp(
    mut source: File,
    mut temp: File,
    temp_path: &Path,
    permissions: Permissions,
    target_path: &Path,
    comments: &[u8],
    pictures: &[(Picture, PictureInformation)],
) -> Result<(), TagError> {
    // rewrite_stream 已经按 256 KiB 分块读写；再套同样大小的 BufReader/BufWriter
    // 只会让每个并发任务多占两份缓冲区，不会减少音频复制的系统调用次数。
    rewrite_stream(&mut source, &mut temp, comments, pictures)?;
    temp.flush()?;
    fs::set_permissions(temp_path, permissions)?;
    // 在暴露新文件名之前把内容和权限刷到存储设备。
    temp.sync_all()?;

    // 先关闭文件再重命名，避免句柄跨过最终提交点，也兼容更严格的平台语义。
    drop(temp);
    drop(source);
    fs::rename(temp_path, target_path)?;
    Ok(())
}

fn create_temp_file(path: &Path) -> Result<(PathBuf, File), TagError> {
    let parent = path.parent().ok_or(TagError::InvalidTagPath)?;
    let file_name = path.file_name().ok_or(TagError::InvalidTagPath)?;

    for _ in 0..100 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = OsString::from(file_name);
        temp_name.push(format!(".tagging-{}-{sequence}.tmp", std::process::id()));
        let temp_path = parent.join(temp_name);
        // 尝试创建临时文件
        match OpenOptions::new()
            // 以可写方式打开
            .write(true)
            // 文件不存在 -> 创建，存在返回already exists
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Err(TagError::CreateTagTemp)
}

fn rewrite_stream<R, W>(
    reader: &mut R,
    writer: &mut W,
    // 接收已经由lofty编码完成的Vorbis Comment字节
    comments: &[u8],
    pictures: &[(Picture, PictureInformation)],
) -> Result<(), TagError>
where
    R: Read,
    W: Write,
{
    // 在堆内存创建一个缓冲区
    let mut copy_buffer = vec![0_u8; COPY_BUFFER_BYTES];
    // 少数工具会在 fLaC 前放一个 ID3v2 标签；该前缀不属于本次编辑范围。
    copy_prefix_and_marker(reader, writer, &mut copy_buffer)?;

    // 逐块扫描旧元数据：删除将要重建的标签、图片和 Padding，其他块（例如
    // SEEKTABLE、CUESHEET、APPLICATION）保持原始字节不变。
    // 记录当前是不是第一个block
    let mut first_block = true;
    // 记录原文件是否存在padding
    let mut saw_padding = false;
    // 记录原文件所有padding总大小
    let mut padding_bytes = 0_u64;
    // 循环读取元数据块
    loop {
        let header = read_block_header(reader)?;
        if first_block
            && (header.block_type != STREAM_INFO_BLOCK || header.length != STREAM_INFO_BYTES)
        {
            return Err(TagError::InvalidFlac(
                "缺少长度为 34 字节的 STREAMINFO 首块",
            ));
        }
        if !first_block && header.block_type == STREAM_INFO_BLOCK {
            return Err(TagError::InvalidFlac("包含重复的 STREAMINFO 块"));
        }
        if header.block_type == INVALID_BLOCK {
            return Err(TagError::InvalidFlac("包含保留的元数据块类型 127"));
        }

        match header.block_type {
            VORBIS_COMMENT_BLOCK | PICTURE_BLOCK => {
                discard_exact(reader, header.length as u64, &mut copy_buffer)?;
            }
            PADDING_BLOCK => {
                saw_padding = true;
                padding_bytes = padding_bytes
                    .checked_add(header.length as u64)
                    .ok_or(TagError::FlacMetadataTooLarge)?;
                discard_exact(reader, header.length as u64, &mut copy_buffer)?;
            }
            _ => {
                write_block_header(writer, false, header.block_type, header.length)?;
                copy_exact(reader, writer, header.length as u64, &mut copy_buffer)?;
            }
        }

        first_block = false;
        if header.is_last {
            break;
        }
    }

    // 新标签和图片直接写入输出流。图片数据不先拼成另一份完整 Vec，避免封面
    // 在序列化阶段额外复制一次。
    write_block_header(writer, false, VORBIS_COMMENT_BLOCK, comments.len())?;
    writer.write_all(comments)?;
    for (picture, information) in pictures {
        write_picture_block(writer, picture, *information)?;
    }

    // 始终让 padding 成为最后一个元数据块，后续工具可在不移动音频帧的情况下扩展标签。
    let padding_bytes = if saw_padding {
        padding_bytes
    } else {
        DEFAULT_PADDING_BYTES
    };
    write_padding(writer, padding_bytes, &mut copy_buffer)?;

    // 此时 reader 正好位于第一个音频帧；剩余内容逐字节复制，不做音频解码。
    copy_to_end(reader, writer, &mut copy_buffer)?;
    Ok(())
}

fn copy_prefix_and_marker<R, W>(
    reader: &mut R,
    writer: &mut W,
    buffer: &mut [u8],
) -> Result<(), TagError>
where
    R: Read,
    W: Write,
{
    let mut first = [0_u8; 4];
    reader.read_exact(&mut first)?;
    if &first == FLAC_MARKER {
        writer.write_all(FLAC_MARKER)?;
        return Ok(());
    }

    // 非标准但常见的 ID3v2 + FLAC 组合：原样保留前置 ID3v2 标签。
    if &first[..3] != b"ID3" {
        return Err(TagError::InvalidFlac("缺少 fLaC 流标记"));
    }
    let mut id3_header = [0_u8; 10];
    id3_header[..4].copy_from_slice(&first);
    reader.read_exact(&mut id3_header[4..])?;
    if id3_header[6..10].iter().any(|byte| byte & 0x80 != 0) {
        return Err(TagError::InvalidFlac("ID3v2 长度不是 synchsafe 整数"));
    }
    let id3_body_bytes = id3_header[6..10]
        .iter()
        .fold(0_u64, |size, byte| (size << 7) | u64::from(*byte));
    let footer_bytes = u64::from(id3_header[3] == 4 && id3_header[5] & 0x10 != 0) * 10;

    writer.write_all(&id3_header)?;
    copy_exact(reader, writer, id3_body_bytes + footer_bytes, buffer)?;

    let mut marker = [0_u8; 4];
    reader.read_exact(&mut marker)?;
    if &marker != FLAC_MARKER {
        return Err(TagError::InvalidFlac("ID3v2 后缺少 fLaC 流标记"));
    }
    writer.write_all(FLAC_MARKER)?;
    Ok(())
}

#[derive(Clone, Copy)]
struct BlockHeader {
    // 每个元数据块头为 1 字节标志/类型 + 3 字节大端长度。
    is_last: bool,
    block_type: u8,
    length: usize,
}

fn read_block_header(reader: &mut impl Read) -> Result<BlockHeader, TagError> {
    let mut header = [0_u8; 4];
    reader.read_exact(&mut header)?;
    Ok(BlockHeader {
        is_last: header[0] & 0x80 != 0,
        block_type: header[0] & 0x7F,
        length: usize::from(header[1]) << 16 | usize::from(header[2]) << 8 | usize::from(header[3]),
    })
}

fn write_block_header(
    writer: &mut impl Write,
    is_last: bool,
    block_type: u8,
    length: usize,
) -> Result<(), TagError> {
    if length > MAX_BLOCK_BYTES {
        return Err(TagError::FlacMetadataTooLarge);
    }
    let length = length as u32;
    writer.write_all(&[
        block_type | (u8::from(is_last) << 7),
        (length >> 16) as u8,
        (length >> 8) as u8,
        length as u8,
    ])?;
    Ok(())
}

fn write_picture_block(
    writer: &mut impl Write,
    picture: &Picture,
    information: PictureInformation,
) -> Result<(), TagError> {
    // PICTURE payload 的整数和长度均为大端 u32。直接向 writer 编码，避免创建
    // “块头 + 图片数据”的第二份完整内存副本。
    let mime = picture.mime_type().map_or("", |value| value.as_str());
    let description = picture.description().unwrap_or("");
    let data = picture.data();
    let payload_len = 4_usize
        .checked_add(4)
        .and_then(|length| length.checked_add(mime.len()))
        .and_then(|length| length.checked_add(4))
        .and_then(|length| length.checked_add(description.len()))
        .and_then(|length| length.checked_add(4 * 4))
        .and_then(|length| length.checked_add(4))
        .and_then(|length| length.checked_add(data.len()))
        .ok_or(TagError::FlacMetadataTooLarge)?;
    if payload_len > MAX_BLOCK_BYTES
        || mime.len() > u32::MAX as usize
        || description.len() > u32::MAX as usize
        || data.len() > u32::MAX as usize
    {
        return Err(TagError::FlacMetadataTooLarge);
    }

    write_block_header(writer, false, PICTURE_BLOCK, payload_len)?;
    write_u32(writer, u32::from(picture.pic_type().as_u8()))?;
    write_sized_bytes(writer, mime.as_bytes())?;
    write_sized_bytes(writer, description.as_bytes())?;
    write_u32(writer, information.width)?;
    write_u32(writer, information.height)?;
    write_u32(writer, information.color_depth)?;
    write_u32(writer, information.num_colors)?;
    write_sized_bytes(writer, data)?;
    Ok(())
}

fn write_sized_bytes(writer: &mut impl Write, value: &[u8]) -> Result<(), TagError> {
    let length = u32::try_from(value.len()).map_err(|_| TagError::FlacMetadataTooLarge)?;
    write_u32(writer, length)?;
    writer.write_all(value)?;
    Ok(())
}

fn write_u32(writer: &mut impl Write, value: u32) -> Result<(), TagError> {
    writer.write_all(&value.to_be_bytes())?;
    Ok(())
}

fn write_padding(
    writer: &mut impl Write,
    padding_bytes: u64,
    buffer: &mut [u8],
) -> Result<(), TagError> {
    buffer.fill(0);
    let mut remaining = padding_bytes;
    loop {
        let chunk = remaining.min(MAX_BLOCK_BYTES as u64) as usize;
        let is_last = remaining <= MAX_BLOCK_BYTES as u64;
        write_block_header(writer, is_last, PADDING_BLOCK, chunk)?;
        write_zeros(writer, chunk, buffer)?;
        if is_last {
            return Ok(());
        }
        remaining -= chunk as u64;
    }
}

fn write_zeros(
    writer: &mut impl Write,
    mut remaining: usize,
    buffer: &[u8],
) -> Result<(), TagError> {
    while remaining > 0 {
        let chunk = remaining.min(buffer.len());
        writer.write_all(&buffer[..chunk])?;
        remaining -= chunk;
    }
    Ok(())
}

fn discard_exact(reader: &mut impl Read, bytes: u64, buffer: &mut [u8]) -> Result<(), TagError> {
    copy_exact(reader, &mut std::io::sink(), bytes, buffer)
}

fn copy_exact(
    reader: &mut impl Read,
    writer: &mut impl Write,
    mut remaining: u64,
    buffer: &mut [u8],
) -> Result<(), TagError> {
    while remaining > 0 {
        let chunk = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        reader.read_exact(&mut buffer[..chunk])?;
        writer.write_all(&buffer[..chunk])?;
        remaining -= chunk as u64;
    }
    Ok(())
}

fn copy_to_end(
    reader: &mut impl Read,
    writer: &mut impl Write,
    buffer: &mut [u8],
) -> Result<(), TagError> {
    loop {
        let read = reader.read(buffer)?;
        if read == 0 {
            return Ok(());
        }
        writer.write_all(&buffer[..read])?;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        time::{SystemTime, UNIX_EPOCH},
    };

    use lofty::{
        config::ParseOptions,
        file::AudioFile,
        flac::FlacFile,
        ogg::{OggPictureStorage, tag::VorbisComments},
        picture::{MimeType, Picture, PictureInformation, PictureType},
        tag::TagExt,
    };

    use super::*;

    const AUDIO_BYTES: &[u8] = b"audio frames are copied byte for byte";

    #[test]
    fn streams_audio_and_replaces_only_tag_blocks() {
        let mut old_comments = VorbisComments::new();
        old_comments.set_vendor("original encoder".to_string());
        old_comments.insert("TITLE".to_string(), "old title".to_string());
        old_comments.insert("CUSTOM".to_string(), "keep me".to_string());
        let input = sample_flac(&old_comments, AUDIO_BYTES);

        let mut new_comments = old_comments.clone();
        new_comments.insert("TITLE".to_string(), "new title".to_string());
        let picture = Picture::unchecked(b"picture bytes".to_vec())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Png)
            .build();
        let pictures = [(picture, PictureInformation::default())];

        let mut encoded_comments = Vec::new();
        new_comments
            .dump_to(&mut encoded_comments, WriteOptions::default())
            .unwrap();
        let mut output = Cursor::new(Vec::new());
        rewrite_stream(
            &mut Cursor::new(input),
            &mut output,
            &encoded_comments,
            &pictures,
        )
        .unwrap();

        let output = output.into_inner();
        assert!(output.ends_with(AUDIO_BYTES));
        let mut parsed = Cursor::new(output);
        let options = ParseOptions::new().read_properties(false);
        let parsed = FlacFile::read_from(&mut parsed, options).unwrap();
        let comments = parsed.vorbis_comments().unwrap();
        assert_eq!(comments.vendor(), "original encoder");
        assert_eq!(comments.get("TITLE"), Some("new title"));
        assert_eq!(comments.get("CUSTOM"), Some("keep me"));
        assert_eq!(parsed.pictures().len(), 1);
    }

    #[test]
    fn rejects_a_missing_stream_info_block() {
        let mut input = Vec::from(FLAC_MARKER);
        input.extend([0x84, 0, 0, 0]);

        let error =
            rewrite_stream(&mut Cursor::new(input), &mut Vec::new(), &[0; 8], &[]).unwrap_err();
        assert!(matches!(error, TagError::InvalidFlac(_)));
    }

    #[test]
    fn preserves_a_leading_id3v2_tag() {
        let comments = VorbisComments::new();
        let mut input = vec![b'I', b'D', b'3', 4, 0, 0, 0, 0, 0, 3, 1, 2, 3];
        input.extend(sample_flac(&comments, AUDIO_BYTES));

        let mut encoded_comments = Vec::new();
        comments
            .dump_to(&mut encoded_comments, WriteOptions::default())
            .unwrap();
        let mut output = Cursor::new(Vec::new());
        rewrite_stream(&mut Cursor::new(input), &mut output, &encoded_comments, &[]).unwrap();

        assert_eq!(
            &output.get_ref()[..13],
            &[b'I', b'D', b'3', 4, 0, 0, 0, 0, 0, 3, 1, 2, 3]
        );
        assert!(output.get_ref().ends_with(AUDIO_BYTES));
    }

    #[test]
    fn atomically_replaces_the_file_and_cleans_the_temporary_path() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "tune-box-flac-writer-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("sample.flac");

        let old_comments = VorbisComments::new();
        fs::write(&path, sample_flac(&old_comments, AUDIO_BYTES)).unwrap();
        let mut new_comments = VorbisComments::new();
        new_comments.insert("TITLE".to_string(), "atomically written".to_string());

        rewrite_flac_tags(&path, &new_comments, &[]).unwrap();

        let bytes = fs::read(&path).unwrap();
        assert!(bytes.ends_with(AUDIO_BYTES));
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn failed_rewrite_keeps_the_original_and_cleans_the_temporary_path() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "tune-box-invalid-flac-writer-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("invalid.flac");
        let original = b"not a FLAC file";
        fs::write(&path, original).unwrap();

        let result = rewrite_flac_tags(&path, &VorbisComments::new(), &[]);

        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    fn sample_flac(comments: &VorbisComments, audio: &[u8]) -> Vec<u8> {
        let mut comment_bytes = Vec::new();
        comments
            .dump_to(&mut comment_bytes, WriteOptions::default())
            .unwrap();

        let mut bytes = Vec::new();
        bytes.extend(FLAC_MARKER);
        write_block_header(&mut bytes, false, STREAM_INFO_BLOCK, STREAM_INFO_BYTES).unwrap();
        bytes.extend([0_u8; STREAM_INFO_BYTES]);
        write_block_header(&mut bytes, false, VORBIS_COMMENT_BLOCK, comment_bytes.len()).unwrap();
        bytes.extend(comment_bytes);
        write_block_header(&mut bytes, true, PADDING_BLOCK, 32).unwrap();
        bytes.extend([0_u8; 32]);
        bytes.extend(audio);
        bytes
    }
}
