import { render, screen, fireEvent } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SearchInput } from '../SearchInput';

describe('SearchInput', () => {
  const defaultProps = {
    searchTerm: '',
    onSearchChange: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders search input with default placeholder', () => {
    render(<SearchInput {...defaultProps} />);

    expect(screen.getByRole('textbox')).toBeInTheDocument();
    expect(screen.getByDisplayValue('')).toBeInTheDocument();
  });

  it('renders search input with custom placeholder', () => {
    const customPlaceholder = 'Custom search placeholder';
    render(<SearchInput {...defaultProps} placeholder={customPlaceholder} />);

    const input = screen.getByRole('textbox');
    expect(input).toHaveAttribute('placeholder', customPlaceholder);
  });

  it('displays current search term', () => {
    render(<SearchInput {...defaultProps} searchTerm="test search" />);

    expect(screen.getByDisplayValue('test search')).toBeInTheDocument();
  });

  it('calls onSearchChange when typing', () => {
    const onSearchChange = vi.fn();
    render(<SearchInput {...defaultProps} onSearchChange={onSearchChange} />);

    const input = screen.getByRole('textbox') as HTMLInputElement;

    // Simulate typing by setting the value and triggering the change event
    input.value = 'new search term';
    fireEvent.change(input);

    expect(onSearchChange).toHaveBeenCalledWith('new search term');
  });

  it('shows clear button when search term is not empty', () => {
    render(<SearchInput {...defaultProps} searchTerm="search text" />);

    // The X icon should be visible
    expect(screen.getByTestId('x-icon')).toBeInTheDocument();
  });

  it('does not show clear button when search term is empty', () => {
    render(<SearchInput {...defaultProps} searchTerm="" />);

    // Should not find X icon (clear button)
    expect(screen.queryByTestId('x-icon')).not.toBeInTheDocument();
  });

  it('calls onSearchChange with empty string when clear button is clicked', () => {
    const onSearchChange = vi.fn();
    render(<SearchInput {...defaultProps} searchTerm="some text" onSearchChange={onSearchChange} />);

    // Find the clear button (X icon)
    const clearButton = screen.getByTestId('x-icon');
    expect(clearButton).toBeInTheDocument();

    const clearButtonContainer = clearButton.parentElement;
    if (!clearButtonContainer) {
      throw new Error('Expected clear button parent element to exist');
    }

    fireEvent.click(clearButtonContainer);
    expect(onSearchChange).toHaveBeenCalledWith('');
  });

  it('has proper accessibility attributes', () => {
    render(<SearchInput {...defaultProps} />);

    const input = screen.getByRole('textbox');
    expect(input).toHaveAttribute('aria-label');
  });

  it('handles multiple input changes correctly', () => {
    const onSearchChange = vi.fn();
    render(<SearchInput {...defaultProps} onSearchChange={onSearchChange} />);

    const input = screen.getByRole('textbox') as HTMLInputElement;

    input.value = 'first';
    fireEvent.change(input);

    input.value = 'second';
    fireEvent.change(input);

    input.value = 'third';
    fireEvent.change(input);

    expect(onSearchChange).toHaveBeenCalledTimes(3);
    expect(onSearchChange).toHaveBeenNthCalledWith(1, 'first');
    expect(onSearchChange).toHaveBeenNthCalledWith(2, 'second');
    expect(onSearchChange).toHaveBeenNthCalledWith(3, 'third');
  });
});
