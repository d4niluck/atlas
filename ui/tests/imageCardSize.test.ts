import test from 'node:test';
import assert from 'node:assert/strict';
import {imageCardWidth, stepImageCardWidth, canvasCardSize} from '../src/imageCardSize.ts';
import {stackFrame} from '../src/fitBoard.ts';

test('image width handles old, invalid and out of bounds data',()=>{
 assert.equal(imageCardWidth(undefined),230);
 assert.equal(imageCardWidth(NaN),230);
 assert.equal(imageCardWidth(120),230);
 assert.equal(imageCardWidth(2000),920);
 assert.equal(stepImageCardWidth(230,-1),230);
 assert.equal(stepImageCardWidth(920,1),920);
});
test('expanded stack frame contains actual image and standard card extents',()=>{
 const cards=[{id:'image',x:100,y:200,width:460,height:390},{id:'note',x:600,y:250}];
 assert.deepEqual(stackFrame(cards,false),{x:80,y:155,w:770,h:453});
 assert.deepEqual(stackFrame(cards,true),{x:80,y:155,w:240,h:72});
});
test('a hidden expanded image reserves its saved width and a safe height before DOM measurement',()=>{
 const image={kind:'file',title:'chart.png',imageExpanded:true,imageWidth:460};
 const size=canvasCardSize(image,null);
 assert.deepEqual(size,{width:460,height:474});
 assert.deepEqual(canvasCardSize(image,{offsetWidth:460,offsetHeight:250}),{width:460,height:250});
 assert.deepEqual(canvasCardSize({...image,imageExpanded:false},null),{width:230,height:82});
});

test('left and right corners keep the opposite edge fixed, including bounds',async()=>{
 const {resizedImageGeometry}=await import('../src/imageCardSize.ts');
 assert.deepEqual(resizedImageGeometry(300,100,-50,'left'),{width:350,x:50});
 assert.deepEqual(resizedImageGeometry(300,100,50,'right'),{width:350,x:100});
 assert.deepEqual(resizedImageGeometry(300,100,500,'left'),{width:230,x:170});
 assert.deepEqual(resizedImageGeometry(900,100,100,'right'),{width:920,x:100});
});

test('vertical motion resizes and top left corner keeps opposite corner fixed',async()=>{
 const {cornerResizeWidth,anchoredImagePosition}=await import('../src/imageCardSize.ts');
 assert.equal(cornerResizeWidth(300,200,0,-40,'left','top'),330);
 assert.equal(cornerResizeWidth(300,200,0,40,'right','bottom'),330);
 assert.deepEqual(anchoredImagePosition(100,200,300,330,200,220,'left','top'),{x:70,y:180});
 assert.deepEqual(anchoredImagePosition(100,200,300,330,200,220,'right','bottom'),{x:100,y:200});
});
