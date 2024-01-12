import { render } from 'preact';
import { LocationProvider, Router, Route } from 'preact-iso';

import { Header } from './components/Header.jsx';
import { Home } from './pages/Home/index.jsx';
import { NotFound } from './pages/_404.jsx';
import './style.css';
import { WgClient } from 'wg-webclient';

const secret = "EMx11sTpRVrReWObruImxwm3rxZMwSJWBqdIJRDPxHM=";
const peer = "AZmudADBwjZIF6vOEDnnzgVPmg/hI987RPllAM1wW2w=";
const wgClient = new WgClient(secret, peer);

export function App() {
	return (
		<LocationProvider>
			<Header />
			<main>
				<Router>
					<Route path="/" component={Home} />
					<Route default component={NotFound} />
				</Router>
			</main>
		</LocationProvider>
	);
}

render(<App />, document.getElementById('app'));
