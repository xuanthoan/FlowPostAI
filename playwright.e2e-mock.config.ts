import baseConfig from './playwright.config';

process.env.E2E_MOCK = 'true';
process.env.E2E_API_KEY = 'mock_dummy_key_for_ci';

export default baseConfig;
