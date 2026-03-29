Add-Type @"
using System;
using System.Runtime.InteropServices;

public class FileSystemWatcher
{
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern IntPtr CreateFileW(
        string lpFileName,
        uint dwDesiredAccess,
        uint dwShareMode,
        IntPtr lpSecurityAttributes,
        uint dwCreationDisposition,
        uint dwFlagsAndAttributes,
        IntPtr hTemplateFile);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool ReadDirectoryChangesW(
        IntPtr hDirectory,
        IntPtr lpBuffer,
        uint nBufferLength,
        bool bWatchSubtree,
        uint dwNotifyFilter,
        out uint lpBytesReturned,
        IntPtr lpOverlapped,
        IntPtr lpCompletionRoutine);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool CloseHandle(IntPtr hObject);

    public const uint FILE_LIST_DIRECTORY = 0x0001;
    public const uint FILE_SHARE_READ = 0x0001;
    public const uint FILE_SHARE_WRITE = 0x0002;
    public const uint FILE_SHARE_DELETE = 0x0004;
    public const uint OPEN_EXISTING = 0x0003;
    public const uint FILE_FLAG_BACKUP_SEMANTICS = 0x02000000;
    public const uint FILE_NOTIFY_CHANGE_FILE_NAME = 0x00000001;
    public const uint FILE_NOTIFY_CHANGE_DIR_NAME = 0x00000002;
    public const uint FILE_NOTIFY_CHANGE_SIZE = 0x00000008;
    public const uint FILE_NOTIFY_CHANGE_LAST_WRITE = 0x00000010;
    public const int INVALID_HANDLE_VALUE = -1;
}
"@

$path = "C:\Users\Administrator\Developer\.git"
$handle = [FileSystemWatcher]::CreateFileW(
    $path,
    [FileSystemWatcher]::FILE_LIST_DIRECTORY,
    [FileSystemWatcher]::FILE_SHARE_READ -bor [FileSystemWatcher]::FILE_SHARE_WRITE -bor [FileSystemWatcher]::FILE_SHARE_DELETE,
    [IntPtr]::Zero,
    [FileSystemWatcher]::OPEN_EXISTING,
    [FileSystemWatcher]::FILE_FLAG_BACKUP_SEMANTICS,
    [IntPtr]::Zero)

if ($handle -eq [FileSystemWatcher]::INVALID_HANDLE_VALUE) {
    Write-Host "エラー: ディレクトリを開けませんでした: $path"
    exit
}

Write-Host "監視を開始しました: $path"
Write-Host "終了するには Ctrl+C を押してください"
Write-Host "---"

$buffer = [System.Runtime.InteropServices.Marshal]::AllocHGlobal(4096)

try {
    while ($true) {
        $bytesReturned = 0
        $result = [FileSystemWatcher]::ReadDirectoryChangesW(
            $handle,
            $buffer,
            4096,
            $false,  # bWatchSubtree
            [FileSystemWatcher]::FILE_NOTIFY_CHANGE_FILE_NAME -bor [FileSystemWatcher]::FILE_NOTIFY_CHANGE_DIR_NAME -bor [FileSystemWatcher]::FILE_NOTIFY_CHANGE_LAST_WRITE,
            [ref]$bytesReturned,
            [IntPtr]::Zero,
            [IntPtr]::Zero)

        if ($result -and $bytesReturned -gt 0) {
            # FILE_NOTIFY_INFORMATION 構造体をパースする
            $offset = 0
            do {
                # NextEntryOffset (4bytes), Action (4bytes), FileNameLength (4bytes), FileName (可変長)
                $nextOffset   = [System.Runtime.InteropServices.Marshal]::ReadInt32($buffer, $offset)
                $action       = [System.Runtime.InteropServices.Marshal]::ReadInt32($buffer, $offset + 4)
                $nameLength   = [System.Runtime.InteropServices.Marshal]::ReadInt32($buffer, $offset + 8)
                $namePtr      = [IntPtr]($buffer.ToInt64() + $offset + 12)
                $fileName     = [System.Runtime.InteropServices.Marshal]::PtrToStringUni($namePtr, $nameLength / 2)

                $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
                switch ($action) {
                    1 { $actionText = "作成" }       # FILE_ACTION_ADDED
                    2 { $actionText = "削除" }       # FILE_ACTION_REMOVED
                    3 { $actionText = "変更" }       # FILE_ACTION_MODIFIED
                    4 { $actionText = "名前変更(旧)" } # FILE_ACTION_RENAMED_OLD_NAME
                    5 { $actionText = "名前変更(新)" } # FILE_ACTION_RENAMED_NEW_NAME
                    default { $actionText = "不明($action)" }
                }
                Write-Host "[$timestamp] [$actionText] $fileName"

                if ($nextOffset -eq 0) { break }
                $offset += $nextOffset
            } while ($true)
        } elseif (-not $result) {
            $err = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
            Write-Host "エラーが発生しました (Win32エラーコード: $err)"
        }
    }
}
finally {
    [System.Runtime.InteropServices.Marshal]::FreeHGlobal($buffer)
    [FileSystemWatcher]::CloseHandle($handle)
}