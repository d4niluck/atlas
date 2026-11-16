import json, os, pathlib, socket, subprocess, sys, tempfile, threading

with tempfile.TemporaryDirectory(prefix='atlas-mode-', dir='/tmp') as t:
    root=pathlib.Path(t); directory=root/'socket';directory.mkdir(mode=0o700)
    path=directory/'agent.sock';server=socket.socket(socket.AF_UNIX);server.bind(str(path));path.chmod(0o600);server.listen()
    key=root/'key';key.write_text('test-agent-key');key.chmod(0o600)
    received=[]
    def serve():
        for _ in range(4):
            conn,_=server.accept()
            with conn:
                data=b''
                while b'\r\n\r\n' not in data:data+=conn.recv(4096)
                head,body=data.split(b'\r\n\r\n',1)
                length=int(next(x.split(b':',1)[1] for x in head.split(b'\r\n') if x.lower().startswith(b'content-length:')))
                while len(body)<length:body+=conn.recv(4096)
                received.append(json.loads(body))
                conn.sendall(b'HTTP/1.1 200 OK\r\nContent-Length: 14\r\nConnection: close\r\n\r\n{"results":[]}')
    worker=threading.Thread(target=serve);worker.start();env=os.environ.copy();env['ATLAS_TEST_AGENT_SOCKET_DIR']=str(directory)
    for mode in ('semantic','hybrid'):
        result=subprocess.run([sys.argv[1],'--key-file',str(key),'--json','search','default','test query',mode],env=env,capture_output=True,text=True,timeout=10)
        assert result.returncode==0,result.stderr
    for scope in ('','abc,def'):
        result=subprocess.run([sys.argv[1],'--key-file',str(key),'--json','--objects',scope,'search','default','test query','semantic'],env=env,capture_output=True,text=True,timeout=10)
        assert result.returncode==0,result.stderr
    worker.join(timeout=5);server.close();assert not worker.is_alive();assert received==[{'query':'test query','offset':0,'limit':10,'mode':'semantic'},{'query':'test query','offset':0,'limit':10,'mode':'hybrid'},{'query':'test query','offset':0,'limit':10,'mode':'semantic','object_ids':[]},{'query':'test query','offset':0,'limit':10,'mode':'semantic','object_ids':['abc','def']}]
print('search mode CLI contract passed')
