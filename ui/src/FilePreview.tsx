/// <reference types="vite/client" />
import {useEffect, useRef, useState} from 'react';
import {getDocument, GlobalWorkerOptions} from './pdfCompat';
import type {PDFDocumentProxy, RenderTask} from 'pdfjs-dist/legacy/build/pdf.mjs';
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
import './file-preview-layout.css';
import {decodePlainText} from './filePreviewText';

GlobalWorkerOptions.workerSrc = workerUrl;

import {readPreviewWindow} from './preview-window';

export function PlainFilePreview({url}: {url: string}) {
  const [value, setValue] = useState<string | null>(null);
  const [status, setStatus] = useState('Загрузка файла…');
  const [truncated, setTruncated] = useState(false);
  useEffect(() => {
    const controller = new AbortController();
    setValue(null); setTruncated(false); setStatus('Загрузка файла…');
    (async () => {
      try {
        const response = await fetch(url, {signal: controller.signal});
        if (!response.ok) throw new Error();
        const part = await readPreviewWindow(response);
        const text = decodePlainText(new TextEncoder().encode(part.text));
        setTruncated(part.truncated);
        if (controller.signal.aborted) return;
        if (text === null) setStatus('Предпросмотр этого файла недоступен. Скачайте его, чтобы открыть.');
        else setValue(text);
      } catch { if (!controller.signal.aborted) setStatus('Не удалось открыть файл.'); }
    })();
    return () => controller.abort();
  }, [url]);
  return value === null ? <p role="status">{status}</p> : <><span role="status">{truncated && "Показано начало большого файла. Полный файл можно скачать."}</span><pre className="file-preview-text">{value}</pre></>;
}

export function PdfPreview({url, title}: {url:string; title:string}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [document, setDocument] = useState<PDFDocumentProxy | null>(null);
  const [page, setPage] = useState(1);
  const [error, setError] = useState('');
  useEffect(() => {
    setDocument(null); setPage(1); setError('');
    const task = getDocument({url});
    let alive = true;
    task.promise.then(pdf => { if (alive) setDocument(pdf); }).catch(() => { if (alive) setError('Не удалось открыть PDF.'); });
    return () => { alive = false; void task.destroy(); };
  }, [url]);
  useEffect(() => {
    if (!document || !canvas.current) return;
    let alive = true;
    let renderTask: RenderTask | null = null;
    document.getPage(page).then(pdfPage => {
      if (!alive || !canvas.current) return;
      const viewport = pdfPage.getViewport({scale: 1.35});
      const target = canvas.current;
      target.width = viewport.width; target.height = viewport.height;
      renderTask = pdfPage.render({canvas: target, viewport});
      return renderTask.promise;
    }).catch(() => { if (alive) setError('Не удалось отобразить страницу PDF.'); });
    return () => { alive = false; renderTask?.cancel(); };
  }, [document, page]);
  return <div className="pdf-preview" aria-label={title}>
    {error ? <p role="alert">{error}</p> : !document ? <p role="status">Открываем PDF…</p> : <>
      <div className="pdf-preview-controls"><button disabled={page <= 1} onClick={() => setPage(page - 1)}>←</button><span>{page} / {document.numPages}</span><button disabled={page >= document.numPages} onClick={() => setPage(page + 1)}>→</button></div>
      <canvas ref={canvas}/>
    </>}
  </div>;
}
