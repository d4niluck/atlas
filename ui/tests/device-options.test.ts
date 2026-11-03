import {test} from 'node:test';
import assert from 'node:assert/strict';
import {availableDevices} from '../src/deviceOptions.ts';
test('Metal is offered only when the backend confirms macOS support',()=>{
 assert.deepEqual(availableDevices(['cpu','metal']),['cpu','metal']);
 assert.deepEqual(availableDevices(['cpu']),['cpu']);
 assert.deepEqual(availableDevices(undefined),['cpu']);
});
