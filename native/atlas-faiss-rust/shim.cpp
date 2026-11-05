#include <faiss/IndexFlat.h>
#include <faiss/index_io.h>
#include <faiss/impl/io.h>
#include <cstring>
#include <faiss/impl/IDSelector.h>
#include <cstdint>
using AtlasRead = size_t (*)(void*, void*, size_t, size_t);
using AtlasWrite = size_t (*)(void*, const void*, size_t, size_t);
struct AtlasReader : faiss::IOReader {
 void* context; AtlasRead callback;
 AtlasReader(void* c, AtlasRead f):context(c),callback(f) {}
 size_t operator()(void* p,size_t size,size_t n) override {return callback(context,p,size,n);}
};
struct AtlasWriter : faiss::IOWriter {
 void* context; AtlasWrite callback;
 AtlasWriter(void* c, AtlasWrite f):context(c),callback(f) {}
 size_t operator()(const void* p,size_t size,size_t n) override {return callback(context,p,size,n);}
};
extern "C" {
int af_write_stream(void* p,void* context,AtlasWrite callback) noexcept {
 try { AtlasWriter w(context,callback);faiss::write_index(static_cast<faiss::Index*>(p),&w);return 0; } catch (...) { return -1; }
}
void* af_read_stream(void* context,AtlasRead callback) noexcept {
 try { AtlasReader r(context,callback);auto* p=faiss::read_index(&r);if(p->d!=512 || !dynamic_cast<faiss::IndexFlatIP*>(p)){delete p;return nullptr;}return p; } catch (...) { return nullptr; }
}

void* af_memory_write(void* p) noexcept {try {auto* writer=new faiss::VectorIOWriter();try{faiss::write_index(static_cast<faiss::Index*>(p),writer);return writer;}catch(...){delete writer;return nullptr;}}catch(...){return nullptr;}}
int64_t af_memory_size(void* w) noexcept {return static_cast<faiss::VectorIOWriter*>(w)->data.size();}
void af_memory_copy(void* w,uint8_t* out) noexcept {auto& d=static_cast<faiss::VectorIOWriter*>(w)->data;std::memcpy(out,d.data(),d.size());}
void af_memory_free(void* w) noexcept {delete static_cast<faiss::VectorIOWriter*>(w);}
void* af_memory_read(const uint8_t* data,int64_t n) noexcept {try {faiss::VectorIOReader r;r.data.assign(data,data+n);auto* p=faiss::read_index(&r);if(p->d!=512 || !dynamic_cast<faiss::IndexFlatIP*>(p)){delete p;return nullptr;}return p;}catch(...){return nullptr;}}
void* af_create() noexcept { try {return new faiss::IndexFlatIP(512);} catch (...) {return nullptr;} }
void af_free(void* p) noexcept { delete static_cast<faiss::Index*>(p); }
int64_t af_count(void* p) noexcept { return static_cast<faiss::Index*>(p)->ntotal; }
int af_reconstruct(void* p,int64_t id,float* out) noexcept {try {auto* index=static_cast<faiss::Index*>(p);if(id<0||id>=index->ntotal)return -1;index->reconstruct(id,out);return 0;} catch (...) {return -1;} }
int af_add(void* p,const float* v,int64_t n) noexcept {try {static_cast<faiss::Index*>(p)->add(n,v);return 0;} catch (...) {return -1;} }
int af_search(void* p,const float* q,const int64_t* ids,int64_t count,int64_t k,float* scores,int64_t* out) noexcept {try {faiss::IDSelectorBatch selector(count,ids); faiss::SearchParameters params; params.sel=&selector; static_cast<faiss::Index*>(p)->search(1,q,k,scores,out,&params);return 0;} catch (...) {return -1;} }
int af_save(void* p,const char* path) noexcept {try {faiss::write_index(static_cast<faiss::Index*>(p),path);return 0;} catch (...) {return -1;} }
void* af_load(const char* path) noexcept {try {auto* p=faiss::read_index(path);if(p->d!=512 || !dynamic_cast<faiss::IndexFlatIP*>(p)){delete p;return nullptr;}return p;} catch (...) {return nullptr;} }
}
