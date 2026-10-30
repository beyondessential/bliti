import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { keepReady } from '../offline.js'
import { loadProtocol } from '../protocol.js'
import '../styles.css'
import Provision from './Provision.jsx'

// Loaded at startup, as the page that reaches a device does, so a page that loaded while online holds
// the module even where the connection drops before the service worker has cached it (WEB).
loadProtocol().catch(() => {})

keepReady()

createRoot(document.getElementById('root')).render(
	<StrictMode>
		<Provision />
	</StrictMode>,
)
