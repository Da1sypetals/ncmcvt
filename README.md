# ncmdump-rs

一个用于解密网易云音乐格式 .ncm (NeteaseCloudMusic Format) 的命令行工具，in **Rust**。

## 使用

### 基本语法

```bash
ncmdump-rs [OPTIONS] <FILES>...
```

### 参数说明

* `<FILES>...` **(必需)**

  需要处理的一个或多个路径。可以是单个 `.ncm` 文件，也可以是包含 `.ncm` 文件的目录。程序会自动遍历目录并处理其中的所有 `.ncm` 文件。

* `-o, --output <OUTPUT>` **(可选)**

  指定输出文件的存放目录。如果未提供此选项，解密后的文件将默认保存在与原始 `.ncm` 文件相同的目录下。

* `-s, --skip` **(可选)**

  如果提供此标志，程序在解密前会检查目标文件是否已存在。如果已存在，则会跳过该文件，避免重复工作。

### 使用示例

1.  **处理单个文件**

    ```bash
    ncmdump-rs "周杰伦 - 七里香.ncm"
    ```

    > 这将在同一个目录下生成 `周杰伦 - 七里香.mp3` (或 `.flac`)。

2.  **处理整个目录下的所有 .ncm 文件**

    ```bash
    ncmdump-rs "/path/to/your/ncm_downloads/"
    ```

    > 程序将递归扫描该目录，并解密所有找到的 `.ncm` 文件。

3.  **处理文件并输出到指定目录**

    ```bash
    ncmdump-rs "歌曲1.ncm" --output "/path/to/my_music/"
    ```

    > 这将在 `/path/to/my_music/` 目录下生成 `歌曲1.mp3`。

4.  **处理文件，并跳过已存在的目标文件**

    ```bash
    ncmdump-rs --skip "/path/to/ncm_downloads/"
    ```

    > 这将解密目录下的所有文件，但如果 `歌曲1.mp3` 已经存在，则会跳过对 `歌曲1.ncm` 的处理。

## 从源码构建

如果你希望自行编译：

```bash
# 1. 克隆仓库
git clone https://github.com/Jel1ySpot/ncmdump-rs.git

# 2. 进入项目目录
cd ncmdump-rs

# 3. 以 release 模式编译
cargo build --release
```
