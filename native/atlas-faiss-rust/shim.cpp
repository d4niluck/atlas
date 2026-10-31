#include <faiss/IndexFlat.h>
#include <faiss/index_io.h>
#include <faiss/impl/IDSelector.h>
#include <cstdint>
extern "C" {
void* af_create() noexcept { try {return new faiss::IndexFlatIP(512);} catch (...) {return nullptr;} }
void af_free(void* p) noexcept { delete static_cast<faiss::Index*>(p); }
int64_t af_count(void* p) noexcept { return static_cast<faiss::Index*>(p)->ntotal; }
int af_reconstruct(void* p,int64_t id,float* out) noexcept {try {auto* index=static_cast<faiss::Index*>(p);if(id<0||id>=index->ntotal)return -1;index->reconstruct(id,out);return 0;} catch (...) {return -1;} }
int af_add(void* p,const float* v,int64_t n) noexcept {try {static_cast<faiss::Index*>(p)->add(n,v);return 0;} catch (...) {return -1;} }
int af_search(void* p,const float* q,const int64_t* ids,int64_t count,int64_t k,float* scores,int64_t* out) noexcept {try {faiss::IDSelectorBatch selector(count,ids); faiss::SearchParameters params; params.sel=&selector; static_cast<faiss::Index*>(p)->search(1,q,k,scores,out,&params);return 0;} catch (...) {return -1;} }
int af_save(void* p,const char* path) noexcept {try {faiss::write_index(static_cast<faiss::Index*>(p),path);return 0;} catch (...) {return -1;} }
void* af_load(const char* path) noexcept {try {auto* p=faiss::read_index(path);if(p->d!=512 || !dynamic_cast<faiss::IndexFlatIP*>(p)){delete p;return nullptr;}return p;} catch (...) {return nullptr;} }
}
