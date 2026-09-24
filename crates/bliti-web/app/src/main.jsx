import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import App from './App.jsx'
import { loadProtocol } from './protocol.js'
import './styles.css'

// Loaded at startup rather than at the first code read, so a page that loaded while online holds the
// module even where the connection drops before the service worker has cached it (WEB). A failure
// here is left for that first read to retry and report.
loadProtocol().catch(() => {})

createRoot(document.getElementById('root')).render(
	<StrictMode>
		<App />
	</StrictMode>,
)
