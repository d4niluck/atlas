#include "fpdfview.h"
#include "fpdf_text.h"
#include <iconv.h>
#include <stdlib.h>
#include <stdio.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <errno.h>
#include <pthread.h>
#include <string.h>
static void *watch_parent(void *unused){(void)unused;char c;ssize_t n;do{n=read(STDIN_FILENO,&c,1);}while(n>0 || (n<0&&errno==EINTR));_exit(14);return NULL;}
typedef struct{int fd;int read_error;} Reader;
static int block(void *arg,unsigned long off,unsigned char *buf,unsigned long len){Reader*r=arg;int fd=r->fd;while(len){ssize_t n=pread(fd,buf,len,off);if(n<0&&errno==EINTR)continue;if(n<=0){r->read_error=1;return 0;}off+=(unsigned long)n;buf+=n;len-=(unsigned long)n;}return 1;}
#ifdef __APPLE__
#define ATLAS_MTIME st_mtimespec
#define ATLAS_CTIME st_ctimespec
#else
#define ATLAS_MTIME st_mtim
#define ATLAS_CTIME st_ctim
#endif
static int unchanged(struct stat*a,struct stat*b){return a->st_dev==b->st_dev&&a->st_ino==b->st_ino&&a->st_size==b->st_size&&a->ATLAS_MTIME.tv_sec==b->ATLAS_MTIME.tv_sec&&a->ATLAS_MTIME.tv_nsec==b->ATLAS_MTIME.tv_nsec&&a->ATLAS_CTIME.tv_sec==b->ATLAS_CTIME.tv_sec&&a->ATLAS_CTIME.tv_nsec==b->ATLAS_CTIME.tv_nsec;}
int main(int argc,char **argv){if(argc!=2 && (argc!=3 || strcmp(argv[2],"--parent-watch")))return 2;if(argc==3){pthread_t tid;if(pthread_create(&tid,NULL,watch_parent,NULL))return 16;pthread_detach(tid);}int fd=open(argv[1],O_RDONLY|O_NOFOLLOW);struct stat before,after;if(fd<0||fstat(fd,&before)||!S_ISREG(before.st_mode)||before.st_size<0)return 10;Reader input={fd,0};FPDF_FILEACCESS access={(unsigned long)before.st_size,block,&input};FPDF_InitLibrary();FPDF_DOCUMENT doc=FPDF_LoadCustomDocument(&access,"");if(!doc){unsigned long error=FPDF_GetLastError();struct stat current;if(input.read_error||fstat(fd,&after)||!unchanged(&before,&after)||lstat(argv[1],&current)||!S_ISREG(current.st_mode)||!unchanged(&before,&current))return 13;return error==FPDF_ERR_PASSWORD?15:3;}int pages=FPDF_GetPageCount(doc);iconv_t conv=iconv_open("UTF-8","UTF-16LE");if(conv==(iconv_t)-1)return 4;for(int i=0;i<pages;i++){if(i && i%256==0){FPDF_CloseDocument(doc);doc=FPDF_LoadCustomDocument(&access,"");if(!doc)return 13;}FPDF_PAGE pg=FPDF_LoadPage(doc,i);if(!pg)return 5;FPDF_TEXTPAGE tp=FPDFText_LoadPage(pg);if(!tp)return 6;int k=FPDFText_CountChars(tp);if(k<0)return 12;if(i && putchar('\n')==EOF)return 9;unsigned short in[8194];char out[32780];unsigned short carry=0;for(int off=0;off<k;){int step=k-off>8192?8192:k-off;int prefix=carry?1:0;if(carry)in[0]=carry;int n=FPDFText_GetText(tp,off,step,in+prefix);if(n<=0||n>step+1)return 12;size_t units=(size_t)(n-1)+prefix;carry=0;if(units&&in[units-1]>=0xd800&&in[units-1]<=0xdbff){carry=in[--units];}char *ip=(char*)in,*op=out;size_t ni=units*2,no=sizeof(out);iconv(conv,NULL,NULL,NULL,NULL);if(iconv(conv,&ip,&ni,&op,&no)==(size_t)-1)return 8;if(fwrite(out,1,(size_t)(op-out),stdout)!=(size_t)(op-out))return 9;off+=step;}if(carry)return 8;FPDFText_ClosePage(tp);FPDF_ClosePage(pg);}iconv_close(conv);FPDF_CloseDocument(doc);FPDF_DestroyLibrary();struct stat current;int ok=fflush(stdout)==0&&!fstat(fd,&after)&&unchanged(&before,&after)&&!lstat(argv[1],&current)&&S_ISREG(current.st_mode)&&unchanged(&before,&current);close(fd);return ok?0:11;}
