import {test} from 'node:test';
import assert from 'node:assert/strict';
import {clampSearchPanelWidth,searchPanelWidthFromPointer} from '../src/searchPanelSizing.ts';
test('search width respects viewport and small windows',()=>{
 assert.equal(clampSearchPanelWidth(1,1000),280);
 assert.equal(clampSearchPanelWidth(900,800),760);
 assert.equal(clampSearchPanelWidth(360,200),160);
 assert.equal(clampSearchPanelWidth(Number.NaN,1000),360);
});
test('dragging left expands right-hand search panel',()=>{
 assert.equal(searchPanelWidthFromPointer(1000,600,1000),400);
 assert.equal(searchPanelWidthFromPointer(1000,400,1000),600);
});
