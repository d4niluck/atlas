import json, os, pathlib, socket, subprocess, sys, tempfile, threading
with tempfile.TemporaryDirectory(prefix='atlas-create-',dir='/tmp') as t:
    root=pathlib.Path(t);directory=root/'socket';directory.mkdir(mode=0o700);path=directory/'agent.sock'
    server=socket.socket(socket.AF_UNIX);server.bind(str(path));path.chmod(0o600);server.listen();key=root/'key';key.write_text('test');key.chmod(0o600)
    text=root/'note.md';text.write_text('Русский текст\nвторая строка'); received=[]
    def serve():
        conn,_=server.accept()
        with conn:
            data=b''
            while b'\r\n\r\n' not in data:data+=conn.recv(4096)
            head,body=data.split(b'\r\n\r\n',1);length=int(next(x.split(b':',1)[1] for x in head.split(b'\r\n') if x.lower().startswith(b'content-length:')))
            while len(body)<length:body+=conn.recv(4096)
            received.append((head.split(b'\r\n')[0],json.loads(body)))
            conn.sendall(b'HTTP/1.1 201 Created\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{"ok":true}')
    worker=threading.Thread(target=serve,daemon=True);worker.start();env=os.environ.copy();env['ATLAS_TEST_AGENT_SOCKET_DIR']=str(directory)
    result=subprocess.run([sys.argv[1],'--key-file',str(key),'--json','create','default','Название',str(text),'task-123'],env=env,capture_output=True,text=True,timeout=10)
    assert result.returncode==0,result.stderr;worker.join(timeout=5);server.close();assert not worker.is_alive()
    assert received==[(b'POST /api/agent/default/notes HTTP/1.1',{'title':'Название','text':'Русский текст\nвторая строка','request_id':'task-123'})]
print('create CLI contract passed')
