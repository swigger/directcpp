#include <stdio.h>
#include <stdint.h>
#include <string>
#include <vector>
#include <regex>
using std::wstring;
using std::string;
using std::vector;

#define extc extern "C"

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <tlhelp32.h>

namespace sutil
{
	template <class CH>
	bool need_quot(const CH* arg) {
		for (const CH* p = arg; *p; ++p) {
			if (isspace(*p) || *p == '"') return true;
		}
		return false;
	}
	template <class SS, class CH>
	void collect_i(SS& o, int ac, const CH** av, const CH* spl)
	{
		for (int i = 0; i < ac; ++i)
		{
			if (need_quot(av[i])) {
				o += '"';
				for (const CH* p = av[i]; *p; ++p)
				{
					if (*p == '\\') {
						for (auto p1 = p + 1; *p1; ++p1) {
							if (*p1 == '\\') continue;
							if (*p1 == '"') {
								o.append(p, p1 - p);
								o.append(p, p1 - p);
								o += '\\';
								o += '\"';
								p = p1;
								break;
							}
							else
							{
								o.append(p, p1 - p);
								p = p1 - 1;
								break;
							}
						}
					}
					else
					{
						if (*p == '"')
							o += '\\';
						o += *p;
					}
				}
				o += '"';
			}
			else
			{
				o += av[i];
			}
			if (i + 1 < ac) o += spl;
		}
	}
	string collect_cmd_ms(int ac, const char** av, const char* spl) {
		string o;
		collect_i(o, ac, av, spl);
		return o;
	}
	wstring collect_cmd_ms(int ac, const wchar_t** av, const wchar_t* spl) {
		wstring o;
		collect_i(o, ac, av, spl);
		return o;
	}

	template <class CH>
	int parse_cmd_i(const CH * src, CH ** oa, int * poan, CH * dst, CH*const dend)
	{
		int olen = 0, oan=0;
		int nchar = -1;
		CH * cur = dst, *pcur = dst;
		enum {NORMAL, INSTR} state = NORMAL;
#define add_char(ch) {if(cur<dend)*cur++=ch; olen += sizeof(CH);}
#define push_cur() do{ if (nchar>=0){\
add_char(0);\
olen+=sizeof(void*);\
if (oa){*oa++ = pcur;} ++oan;\
pcur = cur; nchar=-1;\
}}while(0)

		for (int i=0; ; ++i)
		{
			if (src[i] == 0)
			{
				push_cur();
				cur = 0;
				olen+=sizeof(void*);
				if (oa)*oa++ = 0;
				*poan = oan;
				return olen;
			}
			else if (src[i] == '"')
			{
				if (state == NORMAL)
				{
					if (nchar<0) nchar = 0;
					state = INSTR;
				}
				else if (state == INSTR)
					state = NORMAL;
			}
			else if (src[i]==' ' ||  src[i]=='\t' || src[i]=='\r' || src[i]=='\n')
			{
				if (state == NORMAL)
					push_cur();
				else
					add_char(src[i]);
			}
			else if (src[i] == '\\')
			{
				//ms standard. 2*n \ => n \ ;
				//2*n+1 \ => 2*n+1 \ ;
				int nb = 0;
				for (int j=i; src[j]; ++j)
				{
					if (src[j] == '\\') ++nb;
					else if (src[j]=='"')
					{
						++nb;
						break;
					}
					else break;
				}
				if (nb & 1)
				{
					for (int j=0; j<nb; ++j)
						add_char('\\');
					i += nb;
					--i;
				}
				else
				{
					for (int j=0; j<nb/2; ++j)
						add_char(src[i+2*j+1]);
					i += nb;
					--i;
				}
			}
			else
			{
				if (nchar<0) nchar = 0;
				add_char(src[i]);
			}
		}
	}

	char** parse_cmd_ms(const char * cmd, int * ac)
	{
		int oan = 0;
		int n = parse_cmd_i<char>(cmd, 0, &oan, 0, 0);
		char * omem = (char*)calloc(n+1, 1);
		parse_cmd_i(cmd, (char**)omem, &oan, omem+(oan+1)*sizeof(void*), omem+n);
		*ac = oan;
		return (char**)omem;
	}

	wchar_t** parse_cmd_ms(const wchar_t* cmd, int* ac)
	{
		int oan = 0;
		int n = parse_cmd_i<wchar_t>(cmd, 0, &oan, 0, 0);
		wchar_t* omem = (wchar_t*)calloc(n + 2, 1);
		if (!omem) return nullptr;
		parse_cmd_i<wchar_t>(cmd, (wchar_t**)omem, &oan,
			(wchar_t*)((char*)omem + (oan + 1) * sizeof(void*)),
			(wchar_t*)((char*)omem + n));
		*ac = oan;
		return (wchar_t**)omem;
	}
}

class CLinkHacker
{
	bool m_dirty = false;
	vector<wstring> m_args;
	wstring m_fn;
public:
	CLinkHacker(LPCWSTR fn) {
		m_fn = fn;
		HANDLE hf = CreateFileW(fn, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
		if (hf != INVALID_HANDLE_VALUE) {
			DWORD sz = GetFileSize(hf, NULL);
			wchar_t* mem = (wchar_t*)VirtualAlloc(NULL, sz + 2, MEM_COMMIT, PAGE_READWRITE);
			DWORD rsz;
			BOOL br = ReadFile(hf, mem, sz, &rsz, NULL);
			CloseHandle(hf);
			if (mem && br && mem[0] == 0xfeff) {
				int ac = 0;
				auto argv = sutil::parse_cmd_ms(mem + 1, &ac);
				for (int i = 0; i < ac; ++i) {
					m_args.push_back(argv[i]);
				}
				free(argv);
			}
			VirtualFree(mem, 0, MEM_RELEASE);
		}
	}
	CLinkHacker(int argc, wchar_t** argv) {
		for (int i = 0; i < argc; ++i) {
			m_args.push_back(argv[i]);
		}
	}
	void patch()
	{
		patch_debug();
	}
	wstring collect(const wchar_t* spl) {
		vector<const wchar_t*> args;
		for (auto& ss : m_args) {
			args.push_back(ss.c_str());
		}
		args.push_back(NULL);
		wstring os;
		return sutil::collect_cmd_ms((int)(args.size() - 1), args.data(), spl);
	}
	bool apply()
	{
		if (!m_dirty) return true;
		wstring os;
		os += (wchar_t)0xfeff;
		os += collect(L"\n");
		HANDLE hf = CreateFileW(m_fn.c_str(), GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
		if (hf != INVALID_HANDLE_VALUE)
		{
			DWORD rsz;
			bool b = !! WriteFile(hf, os.c_str(), (DWORD)(os.length() * sizeof(wchar_t)), &rsz, NULL);
			CloseHandle(hf);
			m_dirty = false;
			return b;
		}
		return false;
	}
protected:
	void patch_debug()
	{
		for (auto& ss : m_args) {
			if (wcsicmp(ss.c_str(), L"msvcrt.lib") == 0) {
				ss = L"msvcrtd.lib";
				m_dirty = true;
				break;
			}
		}
	}
};

BOOL (WINAPI *old_CreateProcessW)(LPCWSTR lpApplicationName, LPWSTR lpCommandLine,
	LPSECURITY_ATTRIBUTES lpProcessAttributes, LPSECURITY_ATTRIBUTES lpThreadAttributes,
	BOOL bInheritHandles, DWORD dwCreationFlags, LPVOID lpEnvironment, LPCWSTR lpCurrentDirectory,
	LPSTARTUPINFOW lpStartupInfo, LPPROCESS_INFORMATION lpProcessInformation) = 0;

BOOL WINAPI	myCreateProcessW(LPCWSTR lpApplicationName, LPWSTR lpCommandLine,
	LPSECURITY_ATTRIBUTES lpProcessAttributes, LPSECURITY_ATTRIBUTES lpThreadAttributes,
	BOOL bInheritHandles, DWORD dwCreationFlags, LPVOID lpEnvironment, LPCWSTR lpCurrentDirectory,
	LPSTARTUPINFOW lpStartupInfo, LPPROCESS_INFORMATION lpProcessInformation) {
	if (lpApplicationName) {
		if (wcsstr(lpApplicationName, L"\\link.exe") && lpCommandLine) {
			int argc;
			wchar_t** argv = sutil::parse_cmd_ms(lpCommandLine, &argc);
			if (argc == 2 && argv[1][0] == '@') {
				CLinkHacker hacker(argv[1] + 1);
				free(argv);
				hacker.patch();
				hacker.apply();
			} else if (argc > 2) {
				CLinkHacker hacker(argc, argv);
				free(argv);
				hacker.patch();
				wstring new_cmdline = hacker.collect(L" ");
				return old_CreateProcessW(lpApplicationName, (LPWSTR)new_cmdline.c_str(), lpProcessAttributes, lpThreadAttributes, bInheritHandles,
										dwCreationFlags, lpEnvironment, lpCurrentDirectory, lpStartupInfo, lpProcessInformation);
			}
		}
	}
	return old_CreateProcessW(lpApplicationName, lpCommandLine, lpProcessAttributes, lpThreadAttributes, bInheritHandles,
		dwCreationFlags, lpEnvironment, lpCurrentDirectory, lpStartupInfo, lpProcessInformation);
}

HMODULE FindDll(LPCWSTR regex)
{
	HANDLE hSnapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, 0);
	if (hSnapshot != INVALID_HANDLE_VALUE)
	{
		MODULEENTRY32 me32 = { sizeof(me32) };
		if (Module32First(hSnapshot, &me32))
		{
			do {
				if (std::regex_match(me32.szModule, std::wregex(regex))) {
					CloseHandle(hSnapshot);
					return (HMODULE) me32.modBaseAddr;
				}
			} while (Module32Next(hSnapshot, &me32));
		}
		CloseHandle(hSnapshot);
	}
	return NULL;
}

#define DECL(cls, name, base, off) cls * name = (cls*)((char*)(base) + (INT_PTR)off)

bool FindIAT(HMODULE mod, LPCSTR dname, LPCSTR fname, void*** oPos)
{
	DECL(IMAGE_DOS_HEADER, dosh, mod, 0);
	DECL(IMAGE_NT_HEADERS, nth, mod, dosh->e_lfanew);

	DECL(IMAGE_IMPORT_DESCRIPTOR, des, mod, nth->OptionalHeader.DataDirectory[1].VirtualAddress);
	DWORD sz = (nth->OptionalHeader.DataDirectory[1].Size) / sizeof(IMAGE_THUNK_DATA);

	for (DWORD j = 0; j < sz && des[j].FirstThunk; ++j)
	{
		if (!des[j].OriginalFirstThunk)
		{
			continue;
		}

		DECL(IMAGE_THUNK_DATA, ith, mod, des[j].OriginalFirstThunk);
		DECL(IMAGE_THUNK_DATA, ith2, mod, des[j].FirstThunk);
		DECL(CHAR, dllname, mod, des[j].Name);
		if (dname != 0 && _stricmp(dllname, dname) != 0) continue;
		for (int i = 0; ith[i].u1.Function; ++i)
		{
			auto tmp = ith[i].u1.Ordinal;
			if ((intptr_t)tmp < 0)
			{
				if (!((uintptr_t)fname >> 16))
				{
					tmp = tmp & 0x7fffffff;
					if (tmp == (uintptr_t)fname)
					{
						*oPos = (void**)&ith2[i].u1.Function;
						return true;
					}
				}
			}
			else if ((uintptr_t)fname >> 16)
			{
				DECL(IMAGE_IMPORT_BY_NAME, byn, mod, ith[i].u1.Function);
				if (strcmp((const char*)byn->Name, fname) == 0)
				{
					*oPos = (void**)&ith2[i].u1.Function;
					return true;
				}
			}
		}
	}
	return false;
}

bool write_ptr(void** patchpos, void* v)
{
	if (patchpos)
	{
		DWORD oldpro;
		VirtualProtect(patchpos, sizeof(v), PAGE_EXECUTE_READWRITE, &oldpro);
		BOOL b = WriteProcessMemory(GetCurrentProcess(), patchpos, &v, sizeof(v), 0);
		VirtualProtect(patchpos, sizeof(v), oldpro, &oldpro);
		return !!b;
	}
	return false;
}

/* only for self-debug ////////////////
#pragma comment(lib, "user32.lib")
static void debug_me() {
	char buf[1024];
	uint32_t pid = GetCurrentProcessId();
	sprintf(buf, "pid=%d(%#x)", pid, pid);
	MessageBoxA(0, buf, "debug_me", MB_OK|MB_SERVICE_NOTIFICATION);
}
////////////////////////////////////*/

extc void enable_msvc_debug_c() {
	// already processed?
	if (old_CreateProcessW) return;
	(FARPROC&) old_CreateProcessW = GetProcAddress(GetModuleHandleA("kernel32.dll"), "CreateProcessW");

	HMODULE dll = FindDll(L"std-[0-9A-Fa-f]+.dll");
	void** pos = 0;
	if (dll && FindIAT(dll, "kernel32.dll", "CreateProcessW", &pos)) {
		write_ptr(pos, myCreateProcessW);
	} else {
		dll = GetModuleHandle(0);
		if (FindIAT(dll, "kernel32.dll", "CreateProcessW", &pos)) {
			write_ptr(pos, myCreateProcessW);
		}
	}
}
#else // _WIN32
extc void enable_msvc_debug_c() { /*not windows, nothing to do.*/}
#endif

extc int fmt_g_f64(char *s, size_t len, double value)
{
	return snprintf(s, len, "%.14g", value);
}
