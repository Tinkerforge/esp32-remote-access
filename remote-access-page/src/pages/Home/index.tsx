import { Frame } from '../../components/Frame';
import './style.css';
import Worker from '../../fetch_sw?worker';

function registerServiceWorker() {
	new Worker();
}

registerServiceWorker();

export function Home() {
	return (
		<Frame/>
	);
}
