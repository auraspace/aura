import React, { useState, useEffect } from 'react';
import Layout from '@theme/Layout';
import { useColorMode } from '@docusaurus/theme-common';
import Editor, { Monaco } from '@monaco-editor/react';
import styles from './playground.module.css';
import { registerAuraLanguage } from '../utils/monaco-aura';
import {
  IconPlayerPlay,
  IconShare,
  IconDownload,
  IconTerminal2,
} from '@tabler/icons-react';
import aura_init, { compile, init as init_panic_hook } from '@auraspace/aura';

function PlaygroundContent(): React.ReactNode {
  const { colorMode } = useColorMode();
  const [isLoaded, setIsLoaded] = useState(false);
  const [code, setCode] = useState<string>(
    '// Welcome to the Aura Playground!\nfunction main() {\n    print "Hello, Aura!";\n}\n',
  );
  const [output, setOutput] = useState<string>(
    'Press "Run" to execute your code...\n',
  );

  useEffect(() => {
    async function init() {
      try {
        await aura_init();
        init_panic_hook();
        setIsLoaded(true);
      } catch (err) {
        console.error('Failed to initialize Aura WASM:', err);
        setOutput('Error: Failed to initialize Aura compiler.\n');
      }
    }
    init();
  }, []);

  const handleRun = () => {
    if (!isLoaded) {
      setOutput('Compiler is still loading, please wait...\n');
      return;
    }

    setOutput('Running...\n');

    try {
      const result = compile(code);
      if (result.ok()) {
        const programOutput = result.output();
        setOutput(programOutput || '(Program completed with no output)\n');
      } else {
        const errors = result.errors();
        setOutput(`Execution failed:\n${errors}`);
      }
      // WASM memory management
      result.free();
    } catch (err) {
      setOutput(`Runtime error occurred:\n${err}`);
      console.error(err);
    }
  };

  const handleEditorBeforeMount = (monaco: Monaco) => {
    registerAuraLanguage(monaco);
  };

  return (
    <div className={styles.playgroundContainer}>
      <div className={styles.toolbar}>
        <div className={styles.toolbarLeft}>
          <span className={styles.title}>Aura Playground</span>
        </div>
        <div className={styles.toolbarRight}>
          <button
            className={`${styles.btn} ${styles.btnPrimary}`}
            onClick={handleRun}
          >
            <IconPlayerPlay size={18} />
            Run
          </button>
          <button className={`${styles.btn} ${styles.btnSecondary}`}>
            <IconShare size={18} />
            Share
          </button>
          <button className={`${styles.btn} ${styles.btnSecondary}`}>
            <IconDownload size={18} />
            Save
          </button>
        </div>
      </div>

      <div className={styles.workspace}>
        <div className={styles.editorPane}>
          <div className={styles.paneHeader}>
            <span>main.aura</span>
          </div>
          <div className={styles.editorWrapper}>
            <Editor
              height="100%"
              defaultLanguage="aura"
              beforeMount={handleEditorBeforeMount}
              theme={colorMode === 'dark' ? 'vs-dark' : 'light'}
              value={code}
              onChange={(value) => setCode(value || '')}
              options={{
                minimap: { enabled: false },
                fontSize: 14,
                fontFamily:
                  "'JetBrains Mono', 'Fira Code', 'Courier New', monospace",
                padding: { top: 16 },
              }}
            />
          </div>
        </div>

        <div className={styles.outputPane}>
          <div className={styles.paneHeader}>
            <IconTerminal2 size={16} style={{ marginRight: '8px' }} />
            <span>Console</span>
          </div>
          <div className={styles.terminal}>
            <pre>
              <code>{output}</code>
            </pre>
          </div>
        </div>
      </div>
    </div>
  );
}

export default function Playground(): React.ReactNode {
  return (
    <Layout
      title="Playground"
      description="Write and run Aura code directly in your browser"
    >
      <PlaygroundContent />
    </Layout>
  );
}
