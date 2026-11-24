#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0A00
#endif
#define WIN32_LEAN_AND_MEAN
#pragma warning(disable : 4201)

#include <windows.h>
#include <winternl.h>
#include <stddef.h>
#include <stdint.h>

#define ABI_ASSERT(expression) _Static_assert((expression), #expression)
#define ABI_ALIGN(type) __alignof(type)

/*
 * Compile this probe with the Microsoft x64 compiler and the installed Windows
 * SDK. It checks the SDK declarations independently of the Rust mirror
 * declarations in windows_root_handle.rs.
 */

ABI_ASSERT(sizeof(void *) == 8);
ABI_ASSERT(sizeof(HANDLE) == 8);
ABI_ASSERT(sizeof(ACCESS_MASK) == 4);
ABI_ASSERT(sizeof(NTSTATUS) == 4);

/* UNICODE_STRING: USHORT, USHORT, PWSTR. */
ABI_ASSERT(sizeof(UNICODE_STRING) == 16);
ABI_ASSERT(ABI_ALIGN(UNICODE_STRING) == 8);
ABI_ASSERT(offsetof(UNICODE_STRING, Length) == 0);
ABI_ASSERT(offsetof(UNICODE_STRING, MaximumLength) == 2);
ABI_ASSERT(offsetof(UNICODE_STRING, Buffer) == 8);

/* OBJECT_ATTRIBUTES: ULONG + five pointer/ULONG fields, x64 padding included. */
ABI_ASSERT(sizeof(OBJECT_ATTRIBUTES) == 48);
ABI_ASSERT(ABI_ALIGN(OBJECT_ATTRIBUTES) == 8);
ABI_ASSERT(offsetof(OBJECT_ATTRIBUTES, Length) == 0);
ABI_ASSERT(offsetof(OBJECT_ATTRIBUTES, RootDirectory) == 8);
ABI_ASSERT(offsetof(OBJECT_ATTRIBUTES, ObjectName) == 16);
ABI_ASSERT(offsetof(OBJECT_ATTRIBUTES, Attributes) == 24);
ABI_ASSERT(offsetof(OBJECT_ATTRIBUTES, SecurityDescriptor) == 32);
ABI_ASSERT(offsetof(OBJECT_ATTRIBUTES, SecurityQualityOfService) == 40);

/* IO_STATUS_BLOCK: pointer-sized union followed by ULONG_PTR Information. */
ABI_ASSERT(sizeof(IO_STATUS_BLOCK) == 16);
ABI_ASSERT(ABI_ALIGN(IO_STATUS_BLOCK) == 8);
ABI_ASSERT(offsetof(IO_STATUS_BLOCK, Information) == 8);

/* FILETIME and BY_HANDLE_FILE_INFORMATION. */
ABI_ASSERT(sizeof(FILETIME) == 8);
ABI_ASSERT(ABI_ALIGN(FILETIME) == 4);
ABI_ASSERT(offsetof(FILETIME, dwLowDateTime) == 0);
ABI_ASSERT(offsetof(FILETIME, dwHighDateTime) == 4);

ABI_ASSERT(sizeof(BY_HANDLE_FILE_INFORMATION) == 52);
ABI_ASSERT(ABI_ALIGN(BY_HANDLE_FILE_INFORMATION) == 4);
ABI_ASSERT(offsetof(BY_HANDLE_FILE_INFORMATION, dwFileAttributes) == 0);
ABI_ASSERT(offsetof(BY_HANDLE_FILE_INFORMATION, ftCreationTime) == 4);
ABI_ASSERT(offsetof(BY_HANDLE_FILE_INFORMATION, ftLastAccessTime) == 12);
ABI_ASSERT(offsetof(BY_HANDLE_FILE_INFORMATION, ftLastWriteTime) == 20);
ABI_ASSERT(offsetof(BY_HANDLE_FILE_INFORMATION, dwVolumeSerialNumber) == 28);
ABI_ASSERT(offsetof(BY_HANDLE_FILE_INFORMATION, nFileSizeHigh) == 32);
ABI_ASSERT(offsetof(BY_HANDLE_FILE_INFORMATION, nFileSizeLow) == 36);
ABI_ASSERT(offsetof(BY_HANDLE_FILE_INFORMATION, nNumberOfLinks) == 40);
ABI_ASSERT(offsetof(BY_HANDLE_FILE_INFORMATION, nFileIndexHigh) == 44);
ABI_ASSERT(offsetof(BY_HANDLE_FILE_INFORMATION, nFileIndexLow) == 48);

/* FILE_ID_INFO: ULONGLONG volume serial plus FILE_ID_128. */
ABI_ASSERT(sizeof(FILE_ID_INFO) == 24);
ABI_ASSERT(ABI_ALIGN(FILE_ID_INFO) == 8);
ABI_ASSERT(offsetof(FILE_ID_INFO, VolumeSerialNumber) == 0);
ABI_ASSERT(offsetof(FILE_ID_INFO, FileId) == 8);

/* The SDK header must expose the flag and include it in the valid mask. */
#ifndef OBJ_DONT_REPARSE
#error Windows SDK does not define OBJ_DONT_REPARSE
#endif
#ifndef OBJ_VALID_ATTRIBUTES
#error Windows SDK does not define OBJ_VALID_ATTRIBUTES
#endif
ABI_ASSERT(OBJ_DONT_REPARSE == 0x00001000L);
ABI_ASSERT((OBJ_VALID_ATTRIBUTES & OBJ_DONT_REPARSE) != 0);
ABI_ASSERT(FileIdInfo == 18);

/* Type-check the full user-mode NtCreateFile function pointer signature. */
typedef NTSTATUS (NTAPI *NTCREATEFILE_SIGNATURE)(
    PHANDLE,
    ACCESS_MASK,
    POBJECT_ATTRIBUTES,
    PIO_STATUS_BLOCK,
    PLARGE_INTEGER,
    ULONG,
    ULONG,
    ULONG,
    ULONG,
    PVOID,
    ULONG);

static NTCREATEFILE_SIGNATURE const nt_create_file_signature = &NtCreateFile;

int main(void)
{
    return nt_create_file_signature == NULL;
}
