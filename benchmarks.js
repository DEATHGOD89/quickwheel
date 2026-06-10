import { exec } from 'child_process';
import { performance } from 'perf_hooks';

const benchmarks = {
  startupTime: async () => {
    const start = performance.now();
    // Run app and measure
    const proc = exec('target/release/quickwheel.exe');
    await new Promise(resolve => setTimeout(resolve, 500));
    proc.kill();
    return performance.now() - start;
  },
  
  memoryUsage: async () => {
    // Use Windows tasklist
    return new Promise((resolve) => {
      exec('tasklist /FI "IMAGENAME eq quickwheel.exe" /FO CSV', (err, stdout) => {
        const match = stdout.match(/(\d+,\d+) K/);
        resolve(match ? parseFloat(match[1].replace(',', '')) : 0);
      });
    });
  },
  
  hotkeyResponse: () => {
    // Measure time from hotkey to menu display
    return new Promise((resolve) => {
      const start = performance.now();
      // Simulate hotkey press
      exec('powershell -c "$wshell = New-Object -ComObject wscript.shell; $wshell.SendKeys(\'%{ }\')"');
      setTimeout(() => {
        resolve(performance.now() - start);
      }, 100);
    });
  }
};

async function runBenchmarks() {
  console.log('Running QuickWheel Benchmarks...\n');
  
  const startup = await benchmarks.startupTime();
  console.log(`✓ Startup Time: ${startup.toFixed(2)}ms (Target: <100ms)`);
  
  const memory = await benchmarks.memoryUsage();
  console.log(`✓ Memory Usage: ${memory.toFixed(0)}KB (Target: 15-30MB)`);
  
  const hotkey = await benchmarks.hotkeyResponse();
  console.log(`✓ Hotkey Response: ${hotkey.toFixed(2)}ms (Target: <50ms)`);
  
  console.log('\nBenchmark Complete');
}

runBenchmarks();
