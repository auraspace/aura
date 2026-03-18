import React, { useState } from 'react';
import Layout from '@theme/Layout';
import { useColorMode } from '@docusaurus/theme-common';
import Editor from '@monaco-editor/react';
import styles from './playground.module.css';
import {
  IconPlayerPlay,
  IconShare,
  IconDownload,
  IconTerminal2,
} from '@tabler/icons-react';

function PlaygroundContent(): React.ReactNode {
  const { colorMode } = useColorMode();
  const [code, setCode] = useState<string>(
    '// Welcome to the Aura Playground!\n// WASM compiler integration coming soon.\nfunction main() {\n    println("Hello, Aura!");\n}\n',
  );
  const [output, setOutput] = useState<string>(
    'Compiler not loaded. WASM support will be added later.\n',
  );

  const handleRun = () => {
    setOutput('Running...\n\n(Simulated output) Hello, Aura!\n');
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
