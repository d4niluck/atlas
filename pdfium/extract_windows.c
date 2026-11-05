// Experimental Windows PDFium worker: source is an inherited, already-open file handle on stdin.
#include <windows.h>
#include <io.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include "fpdfview.h"
#include "fpdf_text.h"

typedef struct { HANDLE source; int read_failed; } Reader;
static int get_block(void *arg, unsigned long offset, unsigned char *data, unsigned long length) {
    Reader *r=(Reader *)arg; LARGE_INTEGER pos; pos.QuadPart=offset;
    if (!SetFilePointerEx(r->source,pos,NULL,FILE_BEGIN)) {r->read_failed=1; return 0;}
    while (length) {DWORD n=0;if (!ReadFile(r->source,data,length,&n,NULL) || !n) {r->read_failed=1;return 0;}data+=n; length-=n;}
    return 1;
}
static int write_utf8(unsigned short *text, int length) {
    char bytes[32780];
    int n=WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,(const WCHAR *)text,length,bytes,sizeof(bytes),NULL,NULL);
    return n>0 && fwrite(bytes,1,n,stdout)==(size_t)n;
}
int main(void) {
    if (_setmode(_fileno(stdout),_O_BINARY)<0) return 2;
    HANDLE source=GetStdHandle(STD_INPUT_HANDLE); BY_HANDLE_FILE_INFORMATION before,after;
    if (!source || source==INVALID_HANDLE_VALUE || GetFileType(source)!=FILE_TYPE_DISK ||
        !GetFileInformationByHandle(source,&before) ||
        (before.dwFileAttributes&(FILE_ATTRIBUTE_DIRECTORY|FILE_ATTRIBUTE_REPARSE_POINT)) ||
        before.nFileSizeHigh || before.nFileSizeLow>300UL*1024*1024) return 10;
    Reader reader={source,0}; FPDF_FILEACCESS access={before.nFileSizeLow,get_block,&reader};
    FPDF_InitLibrary(); FPDF_DOCUMENT doc=FPDF_LoadCustomDocument(&access,"");
    if (!doc) {unsigned long err=FPDF_GetLastError(); FPDF_DestroyLibrary(); return reader.read_failed?13:err==FPDF_ERR_PASSWORD?15:3;}
    int count=FPDF_GetPageCount(doc); if(count<0) return 12;
    for(int i=0;i<count;i++){
        if(i && i%256==0){FPDF_CloseDocument(doc);doc=FPDF_LoadCustomDocument(&access,"");if(!doc)return 13;}
        FPDF_PAGE page=FPDF_LoadPage(doc,i);if(!page)return 5;
        FPDF_TEXTPAGE tp=FPDFText_LoadPage(page);if(!tp)return 6;
        int chars=FPDFText_CountChars(tp);if(chars<0)return 12;
        if(i && putchar('\n')==EOF)return 9;
        unsigned short input[8194],carry=0;
        for(int off=0;off<chars;){
            int step=chars-off>8192?8192:chars-off;int prefix=carry?1:0;
            if(carry)input[0]=carry;
            int n=FPDFText_GetText(tp,off,step,input+prefix);if(n<=0||n>step+1)return 12;
            int units=n-1+prefix;carry=0;
            if(units && input[units-1]>=0xd800 && input[units-1]<=0xdbff)carry=input[--units];
            if(units && !write_utf8(input,units))return 8;
            off+=step;
        }
        if(carry)return 8;
        FPDFText_ClosePage(tp);FPDF_ClosePage(page);
    }
    FPDF_CloseDocument(doc);FPDF_DestroyLibrary();
    if (reader.read_failed || !GetFileInformationByHandle(source,&after) ||
        before.dwVolumeSerialNumber!=after.dwVolumeSerialNumber || before.nFileIndexHigh!=after.nFileIndexHigh ||
        before.nFileIndexLow!=after.nFileIndexLow || before.nFileSizeHigh!=after.nFileSizeHigh ||
        before.nFileSizeLow!=after.nFileSizeLow || CompareFileTime(&before.ftLastWriteTime,&after.ftLastWriteTime)!=0) return 11;
    return fflush(stdout)==0?0:9;
}
