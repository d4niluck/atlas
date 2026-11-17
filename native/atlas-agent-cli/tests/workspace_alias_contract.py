import json, os, pathlib, socket, subprocess, sys, tempfile, threading

binary = pathlib.Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='atlas-alias-',dir='/tmp') as tmp:
    root=pathlib.Path(tmp); sockdir=root/'socket'; sockdir.mkdir(mode=0o700)
    sockpath=sockdir/'agent.sock'; assert len(os.fsencode(sockpath)) < 100; server=socket.socket(socket.AF_UNIX);server.bind(str(sockpath));sockpath.chmod(0o600);server.listen();server.settimeout(.2)
    key=root/'key';key.write_text('test');key.chmod(0o600)
    text=root/'note.txt';text.write_text('test text')
    env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir)); stop=threading.Event(); routes=[]; response_mode={'value':'normal'}
    first='11111111111111111111111111111111';second='22222222222222222222222222222222';third='33333333333333333333333333333333';fourth='44444444444444444444444444444444';fifth='55555555555555555555555555555555'
    spaces=[{'id':first,'name':'work','available':True},{'id':second,'name':'Duplicate','available':True},{'id':third,'name':'Duplicate','available':False},{'id':fourth,'name':'Offline','available':False},{'id':fifth,'name':'face','available':True}]
    def worker():
        while not stop.is_set():
            try: conn,_=server.accept()
            except socket.timeout: continue
            except OSError: break
            try:
                data=b''
                while b'\r\n\r\n' not in data:
                    chunk=conn.recv(4096)
                    if not chunk: break
                    data+=chunk
                if not data: continue
                head,_,body=data.partition(b'\r\n\r\n'); lines=head.decode().split('\r\n');method,path,_=lines[0].split(' '); headers=dict(line.split(': ',1) for line in lines[1:] if ': ' in line)
                length=int(headers.get('Content-Length','0'))
                while len(body)<length: body+=conn.recv(4096)
                routes.append((method,path))
                if path=='/api/agent/workspaces':
                    status=403 if response_mode['value']=='forbidden' else 200
                    payload={'error':'Forbidden'} if status==403 else {'workspaces':spaces}
                elif path.endswith('/search') and '/rag/' not in path:status=200;payload={'results':[]}
                elif '/notes/' in path and method=='GET':status=200;payload={'title':'T','text':'test'}
                elif path.endswith('/notes'):status=201;payload={'id':'abc'}
                elif path.endswith('/uploads'):status=201;payload={'replayed':True,'id':'abc'}
                elif '/objects/' in path:status=200;payload={'ok':True}
                elif '/edit' in path:status=200;payload={'lease':'test'}
                elif '/rag/cancel/' in path:status=200;payload={'ok':True}
                elif '/rag/' in path:status=503;payload={'error':'Search mode disabled'}
                else:status=404;payload={'error':'Not found'}
                encoded=(b'{invalid' if path=='/api/agent/workspaces' and response_mode['value']=='invalid-json' else json.dumps(payload).encode());conn.sendall(f'HTTP/1.1 {status} Test\r\nContent-Length: {len(encoded)}\r\nConnection: close\r\n\r\n'.encode()+encoded)
            finally:conn.close()
    thread=threading.Thread(target=worker,daemon=True);thread.start()
    def run(*args):
        routes.clear(); result=subprocess.run([str(binary),'--key-file',str(key),*args],env=env,capture_output=True,text=True,timeout=10)
        return result,list(routes)
    failures=[]
    def case(label,fn):
        try:fn();print('PASS',label,flush=True)
        except Exception as e:failures.append((label,str(e)));print('FAIL',label,str(e),flush=True)
    def success_route(label,args,expected,code=0):
        def check():
            result, seen=run(*args);assert result.returncode==code,(result.stderr,seen);assert any(expected in path for _,path in seen),seen
        case(label,check)
    success_route('search by name',('search','work','q'),f'/api/agent/{first}/search')
    success_route('search by id',('search',first,'q'),f'/api/agent/{first}/search')
    success_route('search default',('search','default','q'),'/api/agent/default/search')
    success_route('read by name',('read','work','a1'),f'/api/agent/{first}/notes/a1')
    success_route('create by name',('create','work','T',str(text),'request-1'),f'/api/agent/{first}/notes')
    success_route('delete by name',('delete','work','a1'),f'/api/agent/{first}/objects/a1/delete')
    success_route('edit by name',('edit','work','a1','acquire'),f'/api/agent/{first}/notes/a1/edit')
    success_route('upload by name',('upload','work',str(text),'request-1'),f'/api/agent/{first}/uploads')
    success_route('cancel by name',('cancel','work','a1'),f'/api/agent/{first}/rag/cancel/a1')
    success_route('rag by name',('rag','work','q'),f'/api/agent/{first}/rag/',1)
    success_route('hex-like workspace name',('search','face','q'),f'/api/agent/{fifth}/search')
    def must_fail_safely(name,needed):
        result,seen=run('search',name,'q');assert result.returncode!=0,(result.stdout,seen);assert all(p=='/api/agent/workspaces' for _,p in seen),seen;assert all(x.lower() in result.stderr.lower() for x in needed),(result.stderr,needed)
    case('ambiguous names do not search',lambda:must_fail_safely('Duplicate',['Duplicate',second,third]))
    case('unknown name has actionable error',lambda:must_fail_safely('Missing',['Missing']))
    case('unavailable name has actionable error',lambda:must_fail_safely('Offline',['Offline']))
    def resolution_error(mode):
        response_mode['value']=mode
        try:
            result,seen=run('create','work','T',str(text),'request-1')
            assert result.returncode!=0,(result.stdout,seen)
            assert seen==[('GET','/api/agent/workspaces')],seen
            assert result.stderr.strip(),result
        finally:response_mode['value']='normal'
    case('forbidden list aborts create',lambda:resolution_error('forbidden'))
    case('malformed JSON list aborts create',lambda:resolution_error('invalid-json'))
    stop.set();server.close();thread.join(timeout=2)
    if failures:
        print('FAILURES:',len(failures),flush=True);sys.exit(1)
    print('All workspace alias cases passed.',flush=True)
