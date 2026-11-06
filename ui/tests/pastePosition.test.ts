import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import {pastePosition} from '../src/pastePosition.ts';
test('paste position is centred within the visible canvas at any pan and zoom',()=>{
 const canvas={clientWidth:1000,clientHeight:800};
 for(const viewport of [{offset:{x:-20000,y:8000},zoom:0.25},{offset:{x:70,y:-300},zoom:2}]){
  const p=pastePosition(canvas,viewport);
  assert.equal((p.x+115)*viewport.zoom+viewport.offset.x,500);
  assert.equal((p.y+41)*viewport.zoom+viewport.offset.y,400);
 }
});
test('avoids a stack covering the centre and stays visible',()=>{
 const p=pastePosition({clientWidth:1000,clientHeight:800},{offset:{x:0,y:0},zoom:1},[{x:250,y:200,w:500,h:400}]);
 assert(p.x+230<=250||p.x>=750||p.y+82<=200||p.y>=600);
 assert(p.x>=0&&p.y>=0&&p.x+230<=1000&&p.y+82<=800);
});
