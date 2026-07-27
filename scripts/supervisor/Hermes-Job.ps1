Set-StrictMode -Version Latest

if (-not ('Hermes.Local.WindowsJob' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;

namespace Hermes.Local
{
    public sealed class WindowsJob : IDisposable
    {
        private IntPtr handle;

        [Flags]
        private enum JobObjectLimitFlags : uint
        {
            KillOnJobClose = 0x00002000
        }

        private enum JobObjectInfoType
        {
            ExtendedLimitInformation = 9
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct IoCounters
        {
            public ulong ReadOperationCount;
            public ulong WriteOperationCount;
            public ulong OtherOperationCount;
            public ulong ReadTransferCount;
            public ulong WriteTransferCount;
            public ulong OtherTransferCount;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct BasicLimitInformation
        {
            public long PerProcessUserTimeLimit;
            public long PerJobUserTimeLimit;
            public JobObjectLimitFlags LimitFlags;
            public UIntPtr MinimumWorkingSetSize;
            public UIntPtr MaximumWorkingSetSize;
            public uint ActiveProcessLimit;
            public UIntPtr Affinity;
            public uint PriorityClass;
            public uint SchedulingClass;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct ExtendedLimitInformation
        {
            public BasicLimitInformation BasicLimitInformation;
            public IoCounters IoInfo;
            public UIntPtr ProcessMemoryLimit;
            public UIntPtr JobMemoryLimit;
            public UIntPtr PeakProcessMemoryUsed;
            public UIntPtr PeakJobMemoryUsed;
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateJobObject(IntPtr securityAttributes, string name);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool SetInformationJobObject(
            IntPtr job,
            JobObjectInfoType infoType,
            IntPtr info,
            uint infoLength);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool CloseHandle(IntPtr handle);

        public WindowsJob(string name)
        {
            handle = CreateJobObject(IntPtr.Zero, name);
            if (handle == IntPtr.Zero)
                throw new Win32Exception(Marshal.GetLastWin32Error(), "CreateJobObject failed");

            var info = new ExtendedLimitInformation();
            info.BasicLimitInformation.LimitFlags = JobObjectLimitFlags.KillOnJobClose;
            int length = Marshal.SizeOf(typeof(ExtendedLimitInformation));
            IntPtr buffer = Marshal.AllocHGlobal(length);
            try
            {
                Marshal.StructureToPtr(info, buffer, false);
                if (!SetInformationJobObject(
                    handle,
                    JobObjectInfoType.ExtendedLimitInformation,
                    buffer,
                    (uint)length))
                {
                    throw new Win32Exception(
                        Marshal.GetLastWin32Error(),
                        "SetInformationJobObject failed");
                }
            }
            finally
            {
                Marshal.FreeHGlobal(buffer);
            }
        }

        public void Assign(Process process)
        {
            if (process == null)
                throw new ArgumentNullException("process");
            if (handle == IntPtr.Zero)
                throw new ObjectDisposedException("WindowsJob");
            if (!AssignProcessToJobObject(handle, process.Handle))
                throw new Win32Exception(
                    Marshal.GetLastWin32Error(),
                    "AssignProcessToJobObject failed");
        }

        public void Dispose()
        {
            if (handle != IntPtr.Zero)
            {
                CloseHandle(handle);
                handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~WindowsJob()
        {
            Dispose();
        }
    }
}
'@
}
