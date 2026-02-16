import { Component, Input } from '@angular/core';
import { CommonModule } from '@angular/common';
import { cn } from '../utils';

@Component({
  selector: 'app-label',
  standalone: true,
  imports: [CommonModule],
  template: `
    <label [for]="htmlFor" [class]="cn('text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70', className)">
      <ng-content></ng-content>
    </label>
  `
})
export class LabelComponent {
  @Input() className: string = '';
  @Input() htmlFor?: string;
  
  cn = cn;
}