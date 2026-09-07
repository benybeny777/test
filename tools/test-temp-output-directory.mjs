import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,mkdir,lstat,symlink,rm,writeFile,readFile} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {createTempOutputDirectory} from './temp-output-directory.mjs';

test('通常の既存階層だけへ新規出力し、逸脱・根・既存出力を拒否する',async()=>{
  const fixture=await mkdtemp(resolve('temp/output-path-test-'));
  try{
    const root=join(fixture,'temp'),parent=join(root,'nested');
    await mkdir(parent,{recursive:true});
    const output=join(parent,'new');
    assert.equal(await createTempOutputDirectory(output,root),output);
    assert.equal((await lstat(output)).isDirectory(),true);
    await writeFile(join(output,'keep.txt'),'保持');
    await assert.rejects(createTempOutputDirectory(output,root),/既存出力/);
    assert.equal(await readFile(join(output,'keep.txt'),'utf8'),'保持');
    await assert.rejects(createTempOutputDirectory(root,root),/temp子/);
    await assert.rejects(createTempOutputDirectory(join(root,'..','escape'),root),/temp子/);
    if(process.platform==='win32'){
      const mixed=join(parent,'case-check').toUpperCase();
      await createTempOutputDirectory(mixed,root.toLowerCase());
      assert.equal((await lstat(mixed)).isDirectory(),true);
    }
  }finally{await rm(fixture,{recursive:true,force:true});}
});

test('実junctionの親・temp根・上位経路を拒否し外へ作成しない',async()=>{
  const fixture=await mkdtemp(resolve('temp/output-junction-test-'));
  try{
    const root=join(fixture,'temp'),outside=join(fixture,'outside');
    await mkdir(root);await mkdir(outside);
    const link=join(root,'redirect');
    await symlink(outside,link,process.platform==='win32'?'junction':'dir');
    await assert.rejects(createTempOutputDirectory(join(link,'new'),root),/リンク|実パス/);
    await assert.rejects(lstat(join(outside,'new')),{code:'ENOENT'});
    await assert.rejects(createTempOutputDirectory(join(link,'new'),link),/リンク|実パス/);
    await mkdir(join(outside,'nested'));
    await assert.rejects(createTempOutputDirectory(join(link,'nested','new'),join(link,'nested')),/リンク|実パス/);
  }finally{await rm(fixture,{recursive:true,force:true});}
});
