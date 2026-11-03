#!/usr/bin/env python3
"""Black-box tests: only own disposable directories; pass Atlas backend path."""
import json, os, pathlib, signal, subprocess, sys, tempfile, unittest
from urllib.request import Request, urlopen, build_opener, ProxyHandler
from urllib.error import HTTPError
opener=build_opener(ProxyHandler({}))
BINARY=os.path.abspath(sys.argv.pop(1))
TOKEN='atlas-isolated-test-token'
PASSWORD='test-password-only'
class AtlasLinux(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory(prefix='atlas-linux-contract-');self.base=pathlib.Path(self.temp.name);self.root=self.base/'registry';flash=os.environ.get('ATLAS_TEST_FLASH_ROOT');self.flash=tempfile.TemporaryDirectory(prefix='atlas-contract-',dir=flash) if flash else None;self.disk=pathlib.Path(self.flash.name) if self.flash else self.base/'disk';self.disk.mkdir(exist_ok=True);self.pc=self.base/'computer';self.pc.mkdir()
  self.proc=subprocess.Popen([BINARY,'--root',str(self.root),'--port','0','--token-stdin'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True)
  self.proc.stdin.write(TOKEN+'\n');self.proc.stdin.flush();port=self.proc.stdout.readline().strip();self.assertTrue(port.isdigit(),f'backend failed: {port}');self.url='http://127.0.0.1:'+port
 def tearDown(self):
  if self.proc.poll() is None:
   os.killpg(self.proc.pid,signal.SIGTERM)
   try:self.proc.wait(timeout=15)
   except subprocess.TimeoutExpired:os.killpg(self.proc.pid,signal.SIGKILL);self.proc.wait()
  if self.flash:self.flash.cleanup()
  self.temp.cleanup()
 def req(self,path,data=None,method=None):
  payload=None if data is None else json.dumps(data).encode();q=Request(self.url+path,data=payload,method=method,headers={'X-Atlas-Token':TOKEN,'Content-Type':'application/json'})
  try:
   with opener.open(q,timeout=90) as r:return r.status,json.load(r)
  except HTTPError as e:return e.code,json.load(e)
 def ok(self,code,result):self.assertEqual(result[0],code,result);return result[1]
 def make(self,protected):
  payload={'name':'isolated workspace','protected':protected}
  if protected:payload['password']=PASSWORD
  wid=self.ok(201,self.req('/api/workspaces',payload))['id'];nid=self.ok(201,self.req('/api/w/'+wid+'/notes',{'title':'test','text':'unique safe note 71'}))['id'];return wid,nid
 def close(self,wid):self.ok(200,self.req('/api/workspaces/'+wid+'/close',{}))
 def cap(self,op,path):return self.ok(200,self.req('/api/workspaces/authorize',{'operation':op,'path':str(path)}))['capability']
 def move(self,wid,dest,protected):
  body={'capability':self.cap('export',dest),'ids':[wid],'move':True}
  if protected:body['passwords']={wid:PASSWORD}
  return self.req('/api/workspaces/export',body)
 def verify_note(self,wid,nid):self.assertEqual(self.ok(200,self.req('/api/w/'+wid+'/notes/'+nid))['text'],'unique safe note 71')
 def test_delete_unlocked(self):
  wid,_=self.make(True);directory=self.root/wid
  self.ok(200,self.req('/api/workspaces/'+wid,method='DELETE'));self.assertFalse(directory.exists())
 def test_delete_locked(self):
  wid,_=self.make(True);directory=self.root/wid;self.close(wid)
  self.ok(200,self.req('/api/workspaces/'+wid,method='DELETE'));self.assertFalse(directory.exists())
 def test_delete_rejects_foreign_file(self):
  wid,nid=self.make(True);self.close(wid);foreign=self.root/wid/'user-file';foreign.write_bytes(b'preserve')
  status,_=self.req('/api/workspaces/'+wid,method='DELETE');self.assertGreaterEqual(status,400);self.assertEqual(foreign.read_bytes(),b'preserve')
  self.ok(200,self.req('/api/workspaces/'+wid+'/unlock',{'password':PASSWORD}));self.verify_note(wid,nid)
 def test_delete_rejects_symlink(self):
  wid,nid=self.make(True);self.close(wid);foreign=self.base/'outside';foreign.write_bytes(b'preserve');(self.root/wid/'unexpected').symlink_to(foreign)
  status,_=self.req('/api/workspaces/'+wid,method='DELETE');self.assertGreaterEqual(status,400);self.assertEqual(foreign.read_bytes(),b'preserve')
  self.ok(200,self.req('/api/workspaces/'+wid+'/unlock',{'password':PASSWORD}));self.verify_note(wid,nid)
 def _roundtrip(self,protected):
  wid,nid=self.make(protected);source=self.root/wid;marker=self.disk/'unrelated';marker.write_bytes(b'keep')
  payload=b'isolated-attachment-7137\x00\xff'*100
  upload=Request(self.url+'/api/w/'+wid+'/files?filename=fixture.bin',data=payload,method='POST',headers={'X-Atlas-Token':TOKEN,'Content-Type':'application/octet-stream'})
  with opener.open(upload,timeout=90) as response:file_id=json.load(response)['id']
  def verify_file(workspace):
   download=Request(self.url+'/api/w/'+workspace+'/files/'+file_id+'/content',headers={'X-Atlas-Token':TOKEN})
   with opener.open(download,timeout=90) as response:self.assertEqual(response.read(),payload)
  if protected:self.close(wid)
  first=self.ok(200,self.move(wid,self.disk,protected));self.assertFalse(source.exists());path=pathlib.Path(first['path']);self.assertTrue(path.exists());self.assertEqual(marker.read_bytes(),b'keep')
  new=first['workspace']['id'];self.verify_note(new,nid);verify_file(new)
  # External workspaces must remain attached; export pauses the protected worker itself.
  second=self.ok(200,self.move(new,self.pc,protected));self.assertFalse((path/'workspace-1').exists());self.assertTrue(pathlib.Path(second['path']).exists());self.assertEqual(marker.read_bytes(),b'keep')
  self.verify_note(second['workspace']['id'],nid);verify_file(second['workspace']['id'])
 def test_move_roundtrip_plain(self):self._roundtrip(False)
 def test_move_roundtrip_protected(self):self._roundtrip(True)
if __name__=='__main__':unittest.main(verbosity=2)
