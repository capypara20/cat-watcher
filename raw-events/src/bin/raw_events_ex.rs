// ReadDirectoryChangesExW 実験ツール
//
// ReadDirectoryChangesExW + ReadDirectoryNotifyExtendedInformation を使い、
// FILE_NOTIFY_EXTENDED_INFORMATION の FileAttributes フィールドで
// ファイル/ディレクトリを削除時でも区別できるか検証する。
//
// 使い方:
//   cargo build -p raw-events --target x86_64-pc-windows-gnu
//   wine target/x86_64-pc-windows-gnu/debug/raw-events-ex.exe 'C:\watch-test'

use chrono::Local;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::ptr;

// ── Windows 型エイリアス ──────────────────────────────────────────────────────
type HANDLE = isize;
type BOOL   = i32;
type DWORD  = u32;
type WCHAR  = u16;

// ── 定数 ─────────────────────────────────────────────────────────────────────
const INVALID_HANDLE_VALUE: HANDLE = -1isize;
const OPEN_EXISTING: DWORD = 3;
const FILE_LIST_DIRECTORY: DWORD = 0x0000_0001;
const FILE_SHARE_READ: DWORD = 0x0000_0001;
const FILE_SHARE_WRITE: DWORD = 0x0000_0002;
const FILE_SHARE_DELETE: DWORD = 0x0000_0004;
const FILE_FLAG_BACKUP_SEMANTICS: DWORD = 0x0200_0000;

const FILE_NOTIFY_CHANGE_FILE_NAME:  DWORD = 0x0000_0001;
const FILE_NOTIFY_CHANGE_DIR_NAME:   DWORD = 0x0000_0002;
const FILE_NOTIFY_CHANGE_ATTRIBUTES: DWORD = 0x0000_0004;
const FILE_NOTIFY_CHANGE_SIZE:       DWORD = 0x0000_0008;
const FILE_NOTIFY_CHANGE_LAST_WRITE: DWORD = 0x0000_0010;
const FILE_NOTIFY_CHANGE_CREATION:   DWORD = 0x0000_0040;

const FILE_ACTION_ADDED:            DWORD = 1;
const FILE_ACTION_REMOVED:          DWORD = 2;
const FILE_ACTION_MODIFIED:         DWORD = 3;
const FILE_ACTION_RENAMED_OLD_NAME: DWORD = 4;
const FILE_ACTION_RENAMED_NEW_NAME: DWORD = 5;

const FILE_ATTRIBUTE_DIRECTORY: DWORD = 0x0000_0010;

// ReadDirectoryNotifyExtendedInformation = 2
const RDNEI: i32 = 2;

// ── FILE_NOTIFY_EXTENDED_INFORMATION レイアウト ───────────────────────────────
// Offset  0: NextEntryOffset  (4)
// Offset  4: Action           (4)
// Offset  8: CreationTime     (8)
// Offset 16: LastModTime      (8)
// Offset 24: LastChangeTime   (8)
// Offset 32: LastAccessTime   (8)
// Offset 40: AllocatedLength  (8)
// Offset 48: FileSize         (8)
// Offset 56: FileAttributes   (4)
// Offset 60: ReparsePointTag  (4)
// Offset 64: FileId           (8)
// Offset 72: ParentFileId     (8)
// Offset 80: FileNameLength   (4)  ← バイト数
// Offset 84: FileName[1]      (2~) ← UTF-16 文字列
const OFF_NEXT:       usize = 0;
const OFF_ACTION:     usize = 4;
const OFF_ATTRS:      usize = 56;
const OFF_NAME_LEN:   usize = 80;
const OFF_NAME:       usize = 84;

// ── extern 宣言 ──────────────────────────────────────────────────────────────
#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        lpFileName:            *const WCHAR,
        dwDesiredAccess:       DWORD,
        dwShareMode:           DWORD,
        lpSecurityAttributes:  *mut u8,
        dwCreationDisposition: DWORD,
        dwFlagsAndAttributes:  DWORD,
        hTemplateFile:         HANDLE,
    ) -> HANDLE;

    fn ReadDirectoryChangesExW(
        hDirectory:                           HANDLE,
        lpBuffer:                             *mut u8,
        nBufferLength:                        DWORD,
        bWatchSubtree:                        BOOL,
        dwNotifyFilter:                       DWORD,
        lpBytesReturned:                      *mut DWORD,
        lpOverlapped:                         *mut u8,
        lpCompletionRoutine:                  *mut u8,
        ReadDirectoryNotifyInformationClass:  i32,
    ) -> BOOL;

    fn CloseHandle(hObject: HANDLE) -> BOOL;
    fn GetLastError() -> DWORD;
}

fn to_wide(s: &str) -> Vec<WCHAR> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| panic!("Usage: raw-events-ex <path>"));

    println!("[start] 監視開始: {path}");
    println!("[start] ReadDirectoryChangesExW + ExtendedInformation");
    println!();

    let wide = to_wide(&path);
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            0,
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        let err = unsafe { GetLastError() };
        eprintln!("CreateFileW 失敗: GetLastError={err}");
        return;
    }

    let filter = FILE_NOTIFY_CHANGE_FILE_NAME
        | FILE_NOTIFY_CHANGE_DIR_NAME
        | FILE_NOTIFY_CHANGE_ATTRIBUTES
        | FILE_NOTIFY_CHANGE_SIZE
        | FILE_NOTIFY_CHANGE_LAST_WRITE
        | FILE_NOTIFY_CHANGE_CREATION;

    let mut buf = vec![0u8; 64 * 1024];
    let mut seq: u64 = 0;

    loop {
        let mut bytes: DWORD = 0;
        let ok = unsafe {
            ReadDirectoryChangesExW(
                handle,
                buf.as_mut_ptr(),
                buf.len() as DWORD,
                1,   // bWatchSubtree = TRUE
                filter,
                &mut bytes,
                ptr::null_mut(),
                ptr::null_mut(),
                RDNEI,
            )
        };

        if ok == 0 {
            let err = unsafe { GetLastError() };
            eprintln!("ReadDirectoryChangesExW 失敗: GetLastError={err}");
            break;
        }
        if bytes == 0 {
            continue;
        }

        let ts = Local::now().format("%H:%M:%S%.3f");
        let mut offset = 0usize;

        loop {
            seq += 1;

            let action     = unsafe { read_u32(&buf, offset + OFF_ACTION) };
            let attrs      = unsafe { read_u32(&buf, offset + OFF_ATTRS) };
            let name_bytes = unsafe { read_u32(&buf, offset + OFF_NAME_LEN) } as usize;
            let name_len   = name_bytes / 2;

            let name_ptr = unsafe {
                buf.as_ptr().add(offset + OFF_NAME) as *const u16
            };
            let name_slice = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
            let name = OsString::from_wide(name_slice);

            let action_str = match action {
                FILE_ACTION_ADDED            => "ADDED",
                FILE_ACTION_REMOVED          => "REMOVED",
                FILE_ACTION_MODIFIED         => "MODIFIED",
                FILE_ACTION_RENAMED_OLD_NAME => "RENAMED_FROM",
                FILE_ACTION_RENAMED_NEW_NAME => "RENAMED_TO",
                _                            => "UNKNOWN",
            };

            let is_dir   = (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0;
            let kind_str = if is_dir { "Dir " } else { "File" };

            println!(
                "[{ts}] #{seq:04} {action_str:<12} kind={kind_str}  attrs=0x{attrs:08X}  {}",
                name.to_string_lossy()
            );

            let next = unsafe { read_u32(&buf, offset + OFF_NEXT) };
            if next == 0 { break; }
            offset += next as usize;
        }
    }

    unsafe { CloseHandle(handle) };
}
