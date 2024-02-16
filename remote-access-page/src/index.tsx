import { render } from 'preact';
import { LocationProvider, Router, Route } from 'preact-iso';

import { Sidebar } from './components/sidebar.js';
import { Home } from './pages/Home/index.jsx';
import { NotFound } from './pages/_404.jsx';
import Container from "react-bootstrap/Container";
import Row from "react-bootstrap/Row";
import 'bootstrap/dist/css/bootstrap.min.css';


export function App() {
    return (
        <LocationProvider>
            <Container fluid>
                <Row>
                    <Sidebar />
                    <main class="col-lg-10 col-md-9 ml-sm-auto px-md-4" >
                        <Router>
                            <Route path="/" component={Home} />
                            <Route default component={NotFound} />
                        </Router>
                    </main>
                </Row>
            </Container>
        </LocationProvider>
    );
}

render(<App />, document.getElementById('app'));
